// Integration tests battering the composite NetworkIdle implementation
// (changes C5 + C6). Each test launches its own Chrome via
// `Browser::launch_chrome(None)` so tests are hermetic and self-contained.
// Compile/run with:
//   cargo test --release --test composite_idle
//
// Test naming:
//   t1_*  Tier 1 — adversarial data: URL pokes
//   t2_*  Tier 2 — local HTTP fixture
//   t3_*  Tier 3 — stress + concurrency
//
// Each test asserts both *correctness* (in-page counter / observable state)
// and a *timing budget* so a hang surfaces as a fast test failure.

mod common;

use std::ops::Deref;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use ferrous_browser::{Browser, BrowserError, WaitUntil};
use tokio::time::timeout;

use common::server::TestServer;

// ─── helpers ────────────────────────────────────────────────────────────────

struct LaunchedBrowser {
    browser: Browser,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl Deref for LaunchedBrowser {
    type Target = Browser;

    fn deref(&self) -> &Self::Target {
        &self.browser
    }
}

fn chrome_test_gate() -> std::sync::Arc<tokio::sync::Semaphore> {
    static GATE: OnceLock<std::sync::Arc<tokio::sync::Semaphore>> = OnceLock::new();
    GATE.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
}

/// Launch Chrome or skip the test if no Chrome binary is on the system.
/// Other errors (timeout, crash) are real test failures.
async fn launch_or_skip() -> Option<LaunchedBrowser> {
    // Each test launches a full Chrome and many assert tight wall-clock
    // budgets. Serialize them at the harness level so default `cargo test`
    // parallelism does not turn host contention into false regressions.
    let permit = chrome_test_gate()
        .acquire_owned()
        .await
        .expect("chrome test semaphore closed");

    match Browser::launch_chrome(None).await {
        Ok(browser) => Some(LaunchedBrowser {
            browser,
            _permit: permit,
        }),
        Err(BrowserError::BrowserNotLaunched(msg)) => {
            eprintln!("⊘ skipping: chrome not launchable: {msg}");
            drop(permit);
            None
        }
        Err(e) => panic!("unexpected chrome launch failure: {e}"),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn data_url(html: &str) -> String {
    format!("data:text/html,{}", urlencode(html))
}

/// Build a *same-origin* page URL on the test server. Necessary whenever the
/// page makes http:// fetches: a `data:` URL is a null/opaque origin and
/// Chrome blocks http:// fetches from it as mixed content / null-origin
/// regardless of CORS headers.
fn host_page(server: &TestServer, html: &str) -> String {
    server.url(&format!("/page?html={}", urlencode(html)))
}

/// Wrap a future in a hard timeout — anything exceeding the budget is a
/// test failure (typically a hang in our composite signal).
async fn within<F, T>(budget: Duration, what: &str, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    match timeout(budget, f).await {
        Ok(v) => v,
        Err(_) => panic!("hard timeout {:?} exceeded: {}", budget, what),
    }
}

/// Time a goto under a hard outer budget. Returns elapsed wall time.
async fn timed_goto(
    page: &ferrous_browser::Page,
    url: &str,
    budget: Duration,
    label: &str,
) -> Duration {
    let t = Instant::now();
    within(budget, label, page.goto(url, WaitUntil::NetworkIdle))
        .await
        .expect("goto returned an error");
    t.elapsed()
}

// ─── Tier 1 ─────────────────────────────────────────────────────────────────

/// T1.1 — 10 parallel fetches kicked off synchronously.
/// Asserts the in_flight counter handles concurrent insertions correctly and
/// idle fires once *all 10* have completed.
#[tokio::test]
async fn t1_parallel_fetches_10() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__started = 0; window.__finished = 0;
for (let i = 0; i < 10; i++) {
  window.__started++;
  fetch('data:text/plain,' + i).then(() => { window.__finished++; });
}
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.1").await;

    let started: u64 = page.evaluate("window.__started").await.unwrap();
    let finished: u64 = page.evaluate("window.__finished").await.unwrap();
    assert_eq!(started, 10);
    assert_eq!(
        finished, 10,
        "all 10 fetches should be observed before idle fires"
    );
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.2 — fetch that 404s (loadingFailed path on Chrome's request counter).
#[tokio::test]
async fn t1_fetch_404() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    // data: URLs always succeed; use a non-resolvable URL instead.
    let html = r#"<!doctype html><html><body><script>
window.__resolved = 0; window.__rejected = 0;
fetch('http://127.0.0.1:1/does-not-exist').then(
  () => window.__resolved++,
  () => window.__rejected++,
);
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.2").await;
    let rejected: u64 = page.evaluate("window.__rejected").await.unwrap();
    assert_eq!(rejected, 1, "failed fetch should have rejected before idle");
    assert!(elapsed < Duration::from_millis(800), "took {elapsed:?}");
}

/// T1.3 — fetch + AbortController.abort(): counter must clean up either via
/// loadingFailed (abort causes a fail) or loadingFinished.
#[tokio::test]
async fn t1_fetch_aborted() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__aborted = 0;
const ctl = new AbortController();
fetch('data:text/plain,abortme', { signal: ctl.signal }).catch(() => { window.__aborted++; });
ctl.abort();
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.3").await;
    let aborted: u64 = page.evaluate("window.__aborted").await.unwrap();
    assert_eq!(aborted, 1);
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.4 — XMLHttpRequest (not fetch): verify Network domain catches it too.
#[tokio::test]
async fn t1_xhr() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__done = 0;
const x = new XMLHttpRequest();
x.open('GET', 'data:text/plain,xhr');
x.onload = () => { window.__done++; };
x.onerror = () => { window.__done++; };
x.send();
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.4").await;
    let done: u64 = page.evaluate("window.__done").await.unwrap();
    assert_eq!(done, 1, "XHR completion should be observed before idle");
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.5 — dynamically inserted <img> with a data: src. Image loads go
/// through the Network domain as well.
#[tokio::test]
async fn t1_dynamic_img() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    // 1x1 transparent PNG as base64 data: URL.
    let html = r#"<!doctype html><html><body><script>
window.__loaded = 0;
const img = new Image();
img.onload = () => { window.__loaded = 1; };
img.src = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.5").await;
    let loaded: u64 = page.evaluate("window.__loaded").await.unwrap();
    assert_eq!(loaded, 1);
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.6 — recursive rAF loop. Our composite waits one rAF; assert we still
/// return promptly even though the page is rAF-looping forever.
#[tokio::test]
async fn t1_recursive_raf() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__rafTicks = 0;
function loop() { window.__rafTicks++; requestAnimationFrame(loop); }
requestAnimationFrame(loop);
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.6").await;
    // Composite should fire on the first rAF; not block on the page's loop.
    assert!(elapsed < Duration::from_millis(150), "took {elapsed:?}");
}

/// T1.7 — rAF callback schedules a fetch. Composite must NOT fire idle until
/// after that fetch has completed.
#[tokio::test]
async fn t1_raf_schedules_fetch() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__fetched = 0;
requestAnimationFrame(() => {
    fetch('data:text/plain,raf-deferred').then(() => { window.__fetched++; });
});
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.7").await;
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    assert_eq!(
        fetched, 1,
        "rAF-scheduled fetch must be observed before idle"
    );
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.8 — setTimeout with delay=0 schedules a deferred fetch. Wrapper should
/// still count it; composite must wait for it.
#[tokio::test]
async fn t1_settimeout_zero() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__fetched = 0;
setTimeout(() => {
    fetch('data:text/plain,zero').then(() => { window.__fetched++; });
}, 0);
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.8").await;
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    assert_eq!(fetched, 1);
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.9 — nested setTimeouts: setTimeout(()=>setTimeout(fetch,200),200).
/// Total expected ≈ 400 ms + fetch + rAF.
#[tokio::test]
async fn t1_nested_settimeouts() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__fetched = 0;
setTimeout(() => {
    setTimeout(() => {
        fetch('data:text/plain,nested').then(() => { window.__fetched++; });
    }, 200);
}, 200);
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(5), "t1.9").await;
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    assert_eq!(
        fetched, 1,
        "nested setTimeout chain must complete before idle"
    );
    assert!(
        elapsed >= Duration::from_millis(400),
        "expected ≥400 ms, took {elapsed:?}"
    );
    assert!(elapsed < Duration::from_millis(800), "took {elapsed:?}");
}

/// T1.10 — clearTimeout race: schedule then immediately cancel.
/// __ferrousPending must not underflow; idle still fires.
#[tokio::test]
async fn t1_cleartimeout_race() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
const id = setTimeout(() => { window.__should_not_fire = true; }, 5000);
clearTimeout(id);
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.10").await;
    let fired: serde_json::Value = page
        .evaluate("typeof window.__should_not_fire === 'undefined' ? null : true")
        .await
        .unwrap();
    assert!(fired.is_null(), "cleared timer must not have fired");
    let pending: i64 = page.evaluate("window.__ferrousPending").await.unwrap();
    assert_eq!(pending, 0, "wrapper counter must not underflow");
    assert!(elapsed < Duration::from_millis(200), "took {elapsed:?}");
}

/// T1.11 — setInterval present. KNOWN GAP: not instrumented today, so the
/// page can issue periodic fetches that the composite signal won't wait for.
/// Test asserts the *current* behaviour with a short outer budget so a
/// future fix (instrumenting setInterval) flips this test rather than
/// silently changing semantics.
#[tokio::test]
async fn t1_setinterval_known_gap() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
window.__ticks = 0;
setInterval(() => { window.__ticks++; }, 100);
</script></body></html>"#;

    // Current behaviour: idle fires promptly because setInterval doesn't
    // increment __ferrousPending. So elapsed should be small (no wait on
    // the interval). When this test starts failing, it means we extended
    // the wrapper to track intervals — at which point this test should be
    // rewritten to assert the new behaviour.
    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.11").await;
    assert!(
        elapsed < Duration::from_millis(200),
        "expected prompt return; took {elapsed:?}"
    );
    let ticks: u64 = page.evaluate("window.__ticks").await.unwrap();
    // No assertion on `ticks` — just documenting the gap.
    eprintln!("ℹ setInterval ticked {} times before goto returned", ticks);
}

/// T1.12 — adversarial setTimeout rebind. Page reassigns window.setTimeout
/// AFTER our wrapper runs at document_start. Our pending counter no longer
/// tracks future calls. Document the failure mode behind a short timeout.
#[tokio::test]
async fn t1_adversarial_rebind_known_gap() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    // Page captures the original setTimeout via an iframe (bypassing our
    // wrapper) and uses it for the deferred fetch.
    let html = r#"<!doctype html><html><body><script>
window.__fetched = 0;
const f = document.createElement('iframe');
document.body.appendChild(f);
const rawST = f.contentWindow.setTimeout;
rawST.call(window, () => {
    fetch('data:text/plain,evaded').then(() => { window.__fetched++; });
}, 200);
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.12").await;
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    // Current behaviour: idle fires before the iframe's setTimeout-driven
    // fetch completes, so the deferred fetch goes unobserved. Don't pin
    // this to a <200 ms wall-clock budget — load+rAF overhead can drift
    // slightly across hosts even when the behavioural gap is still present.
    // When this starts failing, it means we extended the wrapper to cover
    // iframe-borrowed timers.
    assert_eq!(fetched, 0, "expected the evaded fetch to remain unobserved");
    assert!(
        elapsed < Duration::from_secs(1),
        "unexpectedly slow return: {elapsed:?}"
    );
    eprintln!(
        "ℹ adversarial rebind: idle fired with __fetched={} (expected 1 for full coverage)",
        fetched
    );
}

/// T1.13 — iframe with its own setTimeout-deferred fetch. KNOWN GAP:
/// __ferrousPending is per-window; top frame can't see the iframe's pending
/// timers. Document the gap.
#[tokio::test]
async fn t1_iframe_settimeout_known_gap() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let inner =
        r#"<script>setTimeout(() => fetch('data:text/plain,iframe-deferred'), 200);</script>"#;
    let outer_html = format!(
        r#"<!doctype html><html><body>
<iframe srcdoc='{}'></iframe>
</body></html>"#,
        inner.replace('\'', "&apos;")
    );

    let elapsed = timed_goto(
        &page,
        &data_url(&outer_html),
        Duration::from_secs(3),
        "t1.13",
    )
    .await;
    // Idle fires before the iframe's deferred fetch. Top-frame
    // __ferrousPending == 0 even though the iframe has one pending timer.
    eprintln!("ℹ iframe gap: idle fired in {:?}", elapsed);
    // Sanity: timing budget is loose so future fixes that DO wait can pass too.
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
}

/// T1.14 — setTimeout(fn, 0) fires before window.load. Race between sync
/// script's microtask/macrotask and Page.loadEventFired.
#[tokio::test]
async fn t1_settimeout_before_load() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    // Long-tail of timers scheduled before load. They should all be counted
    // and all complete before idle.
    let html = r#"<!doctype html><html><body><script>
window.__fetched = 0;
for (let i = 0; i < 5; i++) {
  setTimeout(() => {
    fetch('data:text/plain,'+i).then(() => { window.__fetched++; });
  }, 0);
}
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.14").await;
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    assert_eq!(fetched, 5);
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

/// T1.15 — document.write content. Different load timing path.
#[tokio::test]
async fn t1_document_write() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body><script>
document.write('<p id="injected">hello</p>');
window.__fetched = 0;
fetch('data:text/plain,after-write').then(() => { window.__fetched++; });
</script></body></html>"#;

    let elapsed = timed_goto(&page, &data_url(html), Duration::from_secs(3), "t1.15").await;
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    let injected: bool = page
        .evaluate("!!document.getElementById('injected')")
        .await
        .unwrap();
    assert!(injected, "document.write content must be present");
    assert_eq!(fetched, 1);
    assert!(elapsed < Duration::from_millis(500), "took {elapsed:?}");
}

// ─── Tier 2 — local HTTP fixture ────────────────────────────────────────────

/// T2.1 — chunked response. Several chunks with gap between them. Counter
/// goes 1 → 0 once the final chunk completes.
#[tokio::test]
async fn t2_chunked_response() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let body_url = server.url("/chunked?n=5&gap=50");
    let html = format!(
        r#"<!doctype html><html><body><script>
window.__bytes = 0;
window.__err = null;
fetch({:?})
  .then(r => r.text())
  .then(t => {{ window.__bytes = t.length; }})
  .catch(e => {{ window.__err = String(e); }});
</script></body></html>"#,
        body_url
    );
    let elapsed = timed_goto(
        &page,
        &host_page(&server, &html),
        Duration::from_secs(5),
        "t2.1",
    )
    .await;
    let debug: String = page
        .evaluate(
            "'bytes='+window.__bytes+' state='+document.readyState+' err='+(window.__err||'none')",
        )
        .await
        .unwrap();
    eprintln!("t2.1 post-goto state: {debug} (elapsed {:?})", elapsed);
    let bytes: u64 = page.evaluate("window.__bytes").await.unwrap();
    assert!(bytes > 0, "chunked body should have been read");
    // 5 chunks × 50 ms gap = ~200 ms; +fetch headers + rAF.
    assert!(elapsed >= Duration::from_millis(200), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

/// T2.2 — stalled body. Server sends headers but never the body. goto must
/// hit its 30 s internal timeout cleanly (Browser-level), not hang past the
/// test budget.
#[tokio::test]
async fn t2_stalled_body() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let body_url = server.url("/stall");
    let html = format!(
        r#"<!doctype html><html><body><script>
fetch({:?});
</script></body></html>"#,
        body_url
    );
    // Hit the goto's outer 30 s with a 35 s test budget; assert it surfaces
    // a Timeout rather than hanging the process.
    let t = Instant::now();
    let res = within(
        Duration::from_secs(35),
        "t2.2 stall goto",
        page.goto(&host_page(&server, &html), WaitUntil::NetworkIdle),
    )
    .await;
    let elapsed = t.elapsed();
    assert!(res.is_err(), "expected goto to time out on stalled fetch");
    assert!(matches!(res, Err(BrowserError::Timeout { .. })));
    assert!(
        elapsed >= Duration::from_secs(29),
        "should have waited the 30 s budget; took {elapsed:?}"
    );
}

/// T2.3 — redirect chain: /redirect?to=/redirect?to=/static
#[tokio::test]
async fn t2_redirect_chain() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let final_url = server.url("/static");
    let mid_url = server.url(&format!("/redirect?to={}", urlencode(&final_url)));
    let start_url = server.url(&format!("/redirect?to={}", urlencode(&mid_url)));

    let elapsed = timed_goto(&page, &start_url, Duration::from_secs(5), "t2.3").await;
    // Confirm we landed on /static by URL.
    let location: String = page.evaluate("location.href").await.unwrap();
    assert!(location.ends_with("/static"), "ended at {location}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

/// T2.4 — HTTP 500 error response from a fetch().
#[tokio::test]
async fn t2_http_500() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let body_url = server.url("/error?status=500");
    let html = format!(
        r#"<!doctype html><html><body><script>
window.__status = 0;
window.__err = null;
fetch({:?})
  .then(r => {{ window.__status = r.status; }})
  .catch(e => {{ window.__err = String(e); }});
</script></body></html>"#,
        body_url
    );
    let elapsed = timed_goto(
        &page,
        &host_page(&server, &html),
        Duration::from_secs(3),
        "t2.4",
    )
    .await;
    let debug: String = page
        .evaluate("'status='+window.__status+' err='+(window.__err||'none')")
        .await
        .unwrap();
    eprintln!("t2.4 post-goto: {debug}");
    let status: u64 = page.evaluate("window.__status").await.unwrap();
    assert_eq!(status, 500);
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

/// T2.5 — realistic mixed page: one slow fetch, two images, one fetch on
/// load. Verifies the composite signal handles a mixed workload cleanly.
#[tokio::test]
async fn t2_realistic_mixed() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let slow = server.url("/slow?ms=120");
    let html = format!(
        r#"<!doctype html><html><body>
<img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=" />
<img src="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=" />
<script>
window.__done = 0;
fetch({:?}).then(() => {{ window.__done++; }});
window.addEventListener('load', () => {{
    fetch('data:text/plain,after-load').then(() => {{ window.__done++; }});
}});
</script>
</body></html>"#,
        slow
    );
    let elapsed = timed_goto(
        &page,
        &host_page(&server, &html),
        Duration::from_secs(5),
        "t2.5",
    )
    .await;
    let done: u64 = page.evaluate("window.__done").await.unwrap();
    assert_eq!(done, 2, "both fetches should have completed");
    // Slow fetch is 120 ms minimum; assert we waited but not too long.
    assert!(elapsed >= Duration::from_millis(120), "{elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
}

/// T2.6 — WebSocket. **Hypothesis**: counter pins at 1 because there is no
/// loadingFinished for an open WebSocket, so goto times out.
/// We host the page same-origin on the test server (`/page?html=...`) so the
/// null-origin WebSocket-from-data:-URL policy isn't a confound.
#[tokio::test]
async fn t2_websocket_known_gap() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let ws_url = format!("ws://{}/ws", server.addr);
    let html = format!(
        r#"<!doctype html><html><body><script>
window.__opened = 0;
const w = new WebSocket({:?});
w.onopen = () => {{ window.__opened++; w.send('hi'); }};
</script></body></html>"#,
        ws_url
    );
    let page_url = server.url(&format!("/page?html={}", urlencode(&html)));

    let t = Instant::now();
    let res = within(
        Duration::from_secs(35),
        "t2.6 ws goto",
        page.goto(&page_url, WaitUntil::NetworkIdle),
    )
    .await;
    let elapsed = t.elapsed();

    // We expect one of two outcomes:
    //   (a) goto times out (hypothesis): counter pinned by WebSocket connection.
    //   (b) goto returns successfully in < 2 s: WebSockets are not counted
    //       by our Network in-flight tracker after all.
    match res {
        Err(BrowserError::Timeout { .. }) => {
            eprintln!(
                "ℹ WebSocket pins NetworkIdle: goto timed out in {:?} as hypothesised",
                elapsed
            );
        }
        Ok(_) if elapsed < Duration::from_secs(2) => {
            // OK: WebSocket open does not pin the counter. Verify the
            // socket actually opened so we know the page ran end-to-end.
            let opened: u64 = page.evaluate("window.__opened").await.unwrap();
            assert!(opened >= 1, "ws.onopen should have fired");
        }
        other => panic!("unexpected outcome from WebSocket goto: {other:?} after {elapsed:?}"),
    }
}

/// T2.7 — Server-Sent Events. A page that opens an `EventSource` must
/// **not** pin NetworkIdle: SSE is a persistent HTTP stream with no
/// terminating `Network.loadingFinished` event, but the composite
/// `update()` now filters `requestWillBeSent` events whose `type` is
/// `"EventSource"` so the in-flight counter never inflates.
#[tokio::test]
async fn t2_sse_does_not_pin() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let sse_url = server.url("/sse");
    let html = format!(
        r#"<!doctype html><html><body><script>
window.__events = 0;
const e = new EventSource({:?});
e.onmessage = () => {{ window.__events++; }};
</script></body></html>"#,
        sse_url
    );

    let t = Instant::now();
    let _ = within(
        Duration::from_secs(5),
        "t2.7 sse goto",
        page.goto(&host_page(&server, &html), WaitUntil::NetworkIdle),
    )
    .await
    .expect("goto should not error");
    let elapsed = t.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "goto should not have been pinned by EventSource; took {elapsed:?}"
    );
    eprintln!("t2.7: SSE goto returned in {:?}", elapsed);
}

// ─── Tier 3 — stress + concurrency ──────────────────────────────────────────

/// T3.1 — N parallel goto's on different Pages. Asserts session-id-based
/// partitioning in the broadcast channel keeps each Page's in_flight
/// tracking independent.
///
/// We deliberately use *data: URLs with a 100 ms setTimeout-driven fetch*
/// (not an HTTP server) so the test exercises our event partitioning and
/// not Chrome's per-origin connection pool — those two confound otherwise.
#[tokio::test]
async fn t3_parallel_pages() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let mut pages = Vec::new();
    for _ in 0..5 {
        pages.push(browser.new_page().await.unwrap());
    }

    // Each page does setTimeout(100) → fetch(data:) — the wrapper's timer
    // counter and the Network in-flight counter both get exercised.
    let html = r#"<!doctype html><html><body><script>
window.__done = 0;
setTimeout(() => {
    fetch('data:text/plain,parallel').then(() => { window.__done++; });
}, 100);
</script></body></html>"#;
    let url = data_url(html);

    let t = Instant::now();
    let mut handles = Vec::new();
    for p in &pages {
        let p2 = p.clone();
        let u2 = url.clone();
        handles.push(tokio::spawn(async move {
            p2.goto(&u2, WaitUntil::NetworkIdle).await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let elapsed = t.elapsed();
    for p in &pages {
        let done: u64 = p.evaluate("window.__done").await.unwrap();
        assert_eq!(done, 1, "each page should have completed its fetch");
    }
    eprintln!("t3.1: 5 parallel gotos in {:?}", elapsed);
    assert!(elapsed < Duration::from_millis(1500), "took {elapsed:?}");
}

/// T3.2 — Two sequential gotos on the same Page. Second goto should not be
/// polluted by pending state from the first.
#[tokio::test]
async fn t3_sequential_same_page() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let a = data_url(
        r#"<!doctype html><html><body><script>
setTimeout(() => fetch('data:text/plain,a-late'), 5000); // would-be pending
fetch('data:text/plain,a-now');
</script></body></html>"#,
    );
    let b = data_url(
        r#"<!doctype html><html><body><script>
window.__fetched = 0;
fetch('data:text/plain,b').then(() => { window.__fetched++; });
</script></body></html>"#,
    );

    // First goto MAY still be waiting for the 5 s setTimeout — wrap it in
    // a budget that proves we noticed the pending timer (it shouldn't
    // return in <5 s).
    let t1 = Instant::now();
    let _ = within(
        Duration::from_secs(8),
        "t3.2-a",
        page.goto(&a, WaitUntil::NetworkIdle),
    )
    .await;
    let a_elapsed = t1.elapsed();
    assert!(
        a_elapsed >= Duration::from_secs(5),
        "first goto should have waited for the 5s timer; took {a_elapsed:?}"
    );

    // Second goto on a fresh page should be fast — the new document has a
    // fresh wrapper and no inherited pending state.
    let t2 = Instant::now();
    let _ = within(
        Duration::from_secs(3),
        "t3.2-b",
        page.goto(&b, WaitUntil::NetworkIdle),
    )
    .await;
    let b_elapsed = t2.elapsed();
    let fetched: u64 = page.evaluate("window.__fetched").await.unwrap();
    assert_eq!(fetched, 1);
    assert!(
        b_elapsed < Duration::from_millis(500),
        "second goto polluted; took {b_elapsed:?}"
    );
}

/// T3.3 — 1 000 sequential gotos as a memory-leak canary.
/// Checks RSS delta from /proc/self/status before/after.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn t3_thousand_sequential_gotos() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let url = data_url("<!doctype html><html><body>leak-canary</body></html>");

    let rss_before = read_rss_kb();
    let t = Instant::now();
    for i in 0..1000 {
        within(
            Duration::from_secs(5),
            &format!("t3.3 iter {i}"),
            page.goto(&url, WaitUntil::NetworkIdle),
        )
        .await
        .unwrap();
    }
    let elapsed = t.elapsed();
    let rss_after = read_rss_kb();

    eprintln!(
        "t3.3: 1000 gotos in {:?}; RSS {} → {} kB (Δ {} kB)",
        elapsed,
        rss_before,
        rss_after,
        (rss_after as i64) - (rss_before as i64),
    );

    let delta_kb = (rss_after as i64) - (rss_before as i64);
    // 20 MB is generous; typical run should be well under.
    assert!(
        delta_kb < 20_000,
        "RSS grew by {delta_kb} kB across 1000 gotos — likely a leak"
    );
}

#[cfg(target_os = "linux")]
fn read_rss_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            // VmRSS:     12345 kB
            let n = rest
                .split_whitespace()
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0);
            return n;
        }
    }
    0
}

/// T3.4 — CDP disconnect mid-goto. Drop the Browser handle while a goto is
/// hanging on a stalled fetch; the goto must surface the disconnect within
/// a small bound. Both wait paths in `goto` now match
/// `RecvError::Closed` explicitly and return `BrowserError::websocket(..)`
/// rather than waiting out the outer 30 s timeout.
#[tokio::test]
async fn t3_cdp_disconnect_midgoto() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let server = TestServer::spawn().await;
    let page = browser.new_page().await.unwrap();

    let stall = server.url("/stall");
    let html = format!(
        r#"<!doctype html><html><body><script>
fetch({:?});
</script></body></html>"#,
        stall
    );

    let url = host_page(&server, &html);
    let page2 = page.clone();
    let goto_handle = tokio::spawn(async move { page2.goto(&url, WaitUntil::NetworkIdle).await });

    // Give the goto a moment to start (Page.navigate fires, /page loads,
    // /stall fetch starts and pins the counter), then drop the browser.
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(browser);

    let t = Instant::now();
    let res = within(Duration::from_secs(5), "t3.4", goto_handle)
        .await
        .unwrap();
    let elapsed = t.elapsed();
    assert!(res.is_err(), "expected goto to fail after browser drop");
    assert!(
        elapsed < Duration::from_secs(3),
        "disconnect should surface within 3 s; took {elapsed:?}"
    );
    eprintln!("t3.4: goto failed in {:?} with {:?}", elapsed, res.err());
}
