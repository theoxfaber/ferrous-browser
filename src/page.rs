use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tracing::Instrument;

use crate::cdp::CDPClient;
use crate::error::{BrowserError, Result};

// ─── P2: WaitUntil enum ──────────────────────────────────────────────────────

/// Controls when [`Page::goto`] considers navigation complete.
#[derive(Debug, Clone, Copy, Default)]
pub enum WaitUntil {
    /// Wait for `Page.domContentEventFired` — the DOM is parsed but
    /// sub-resources (images, stylesheets) may still be loading.
    DomContentLoaded,
    /// Wait for `Page.loadEventFired` — all resources have loaded.
    /// This is the default.
    #[default]
    Load,
    /// Wait until there are no in-flight network requests for 500 ms.
    /// Useful for SPAs that fetch data after the load event.
    NetworkIdle,
}

// ─── P2B: Cookie ─────────────────────────────────────────────────────────────

/// Represents a browser cookie for session persistence.
///
/// # Example
///
/// ```no_run
/// # use ferrous_browser::{Browser, Cookie, WaitUntil};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let browser = Browser::launch().await?;
/// let page = browser.new_page().await?;
/// let cookies = vec![Cookie {
///     name: "session".to_string(),
///     value: "abc123".to_string(),
///     ..Default::default()
/// }];
/// page.set_cookies(&cookies).await?;
/// let retrieved = page.cookies().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cookie {
    /// Cookie name
    pub name: String,
    /// Cookie value
    pub value: String,
    /// Cookie domain (default: page domain)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Cookie path (default: "/")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Seconds since epoch when cookie expires (default: session cookie)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<f64>,
    /// HTTPS only flag
    #[serde(default)]
    pub secure: bool,
    /// HTTP only flag (not accessible via JavaScript)
    #[serde(default, rename = "httpOnly")]
    pub http_only: bool,
    /// SameSite attribute ("Strict", "Lax", "None")
    #[serde(skip_serializing_if = "Option::is_none", rename = "sameSite")]
    pub same_site: Option<String>,
}

// ─── P3: Locator ─────────────────────────────────────────────────────────────

/// A lazy handle to a DOM element identified by a CSS selector.
///
/// Locators are created with [`Page::locator`] and make the common
/// "find-then-act" pattern ergonomic and composable.
///
/// # Example
///
/// ```no_run
/// # use ferrous_browser::{Browser, WaitUntil};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let browser = Browser::launch().await?;
/// let page = browser.new_page().await?;
/// page.goto("https://example.com", WaitUntil::Load).await?;
///
/// // Locator API
/// page.locator("button#submit").click().await?;
/// page.locator("input[name=q]").type_text("hello").await?;
/// page.locator(".result").wait_for().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Locator {
    selector: String,
    page: Page,
}

impl Locator {
    fn new(selector: impl Into<String>, page: Page) -> Self {
        Self {
            selector: selector.into(),
            page,
        }
    }

    /// Click the element identified by this locator.
    pub async fn click(&self) -> Result<()> {
        self.page.click_selector(&self.selector).await
    }

    /// Type text into the element identified by this locator.
    pub async fn type_text(&self, text: &str) -> Result<()> {
        self.page.type_text_selector(&self.selector, text).await
    }

    /// Wait until the element is present in the DOM (30 s default timeout).
    pub async fn wait_for(&self) -> Result<()> {
        self.page.wait_for_selector(&self.selector).await
    }

    /// Wait until the element is present with a custom timeout.
    pub async fn wait_for_timeout(&self, dur: Duration) -> Result<()> {
        self.page
            .wait_for_selector_with_timeout(&self.selector, dur)
            .await
    }

    /// Wait for the element to become actionable (attached, not disabled,
    /// visible, non-zero size) and then click it. The entire wait + click
    /// happens inside the page in one CDP round-trip, driven by a
    /// MutationObserver, so reaction latency is bounded by the
    /// attribute/childList mutation rather than by a polling cadence.
    pub async fn click_auto(&self) -> Result<()> {
        self.click_auto_with_timeout(Duration::from_secs(30)).await
    }

    /// [`Locator::click_auto`] with a custom timeout.
    pub async fn click_auto_with_timeout(&self, dur: Duration) -> Result<()> {
        self.page
            .click_selector_auto_wait(&self.selector, dur)
            .await
    }

    /// Get the inner text of the element.
    pub async fn inner_text(&self) -> Result<String> {
        let expr = format!(
            "document.querySelector('{}')?.innerText ?? ''",
            escape_selector(&self.selector)
        );
        let result = self
            .page
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({ "expression": expr, "returnByValue": true })),
            )
            .await?;
        result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BrowserError::invalid_response(
                    format!("inner_text('{}')", self.selector),
                    "unexpected result shape",
                )
            })
    }

    /// Get an attribute value of the element.
    pub async fn get_attribute(&self, name: &str) -> Result<Option<String>> {
        let expr = format!(
            "document.querySelector('{}')?.getAttribute('{}') ?? null",
            escape_selector(&self.selector),
            name,
        );
        let result = self
            .page
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({ "expression": expr, "returnByValue": true })),
            )
            .await?;
        let val = result.get("result").and_then(|r| r.get("value"));
        match val {
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(Value::Null) | None => Ok(None),
            _ => Ok(val.map(|v| v.to_string())),
        }
    }
}

// ─── Page ────────────────────────────────────────────────────────────────────

/// A handle to a single page/tab in the browser.
///
/// Page provides methods for interacting with a specific page or tab,
/// including navigation, content retrieval, screenshot capture, and
/// element interaction.
///
/// # Example
///
/// ```no_run
/// use ferrous_browser::{Browser, WaitUntil};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let browser = Browser::launch().await?;
/// let page = browser.new_page().await?;
///
/// page.goto("https://example.com", WaitUntil::Load).await?;
/// let html = page.content().await?;
/// let screenshot = page.screenshot().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Page {
    /// Target/page ID
    pub target_id: String,
    /// Session ID for routing CDP commands
    pub session_id: String,
    /// Reference to CDP client
    cdp: Arc<CDPClient>,
    /// Lazily-enabled Page domain: the first `goto` (or anything else that
    /// needs Page events) performs the one-time `Page.enable` round-trip.
    /// Unlike a plain OnceCell, failures are not cached forever: a transient
    /// CDP hiccup leaves the page retryable on the next call.
    page_enabled: Arc<InitGuard>,
    /// Same retryable-init pattern for `Network.enable`. Today this is only
    /// needed by the composite NetworkIdle implementation.
    network_enabled: Arc<InitGuard>,
    /// One-shot install of the document_start setTimeout/clearTimeout wrapper
    /// so the composite NetworkIdle signal can wait on the pending-timers
    /// counter as well as network in-flight count.
    timer_script_injected: Arc<InitGuard>,
}

struct InitGuard {
    ready: AtomicBool,
    lock: tokio::sync::Mutex<()>,
}

impl InitGuard {
    fn new() -> Self {
        Self {
            ready: AtomicBool::new(false),
            lock: tokio::sync::Mutex::new(()),
        }
    }

    async fn ensure<F, Fut>(&self, init: F) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<()>>,
    {
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }

        let _guard = self.lock.lock().await;
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }

        init().await?;
        self.ready.store(true, Ordering::Release);
        Ok(())
    }
}

impl Page {
    /// Create a new page handle
    #[doc(hidden)]
    pub fn new(target_id: String, session_id: String, cdp: Arc<CDPClient>) -> Self {
        Page {
            target_id,
            session_id,
            cdp,
            page_enabled: Arc::new(InitGuard::new()),
            network_enabled: Arc::new(InitGuard::new()),
            timer_script_injected: Arc::new(InitGuard::new()),
        }
    }

    /// Ensure the Page domain is enabled on this session. Cheap on every
    /// call after the first: once the init succeeds, subsequent callers
    /// return synchronously. Failed attempts do not poison the page.
    async fn ensure_page_enabled(&self) -> Result<()> {
        let cdp = self.cdp.clone();
        let sid = self.session_id.clone();
        self.page_enabled
            .ensure(move || async move {
                cdp.send_command_with_session(&sid, "Page.enable".to_string(), None)
                    .await
                    .map(|_| ())
            })
            .await?;
        Ok(())
    }

    /// Same retryable-init pattern for `Network.enable` so subsequent
    /// `WaitUntil::NetworkIdle` calls don't re-pay the round-trip.
    async fn ensure_network_enabled(&self) -> Result<()> {
        let cdp = self.cdp.clone();
        let sid = self.session_id.clone();
        self.network_enabled
            .ensure(move || async move {
                cdp.send_command_with_session(&sid, "Network.enable".to_string(), None)
                    .await
                    .map(|_| ())
            })
            .await?;
        Ok(())
    }

    /// Install a document_start wrapper around `setTimeout`/`clearTimeout`
    /// that maintains `window.__ferrousPending` (an integer count of
    /// scheduled-but-not-yet-fired timers) and `window.__ferrousAwaitTimers`
    /// (a Promise that resolves the next moment the pending count hits 0).
    /// The composite NetworkIdle flush awaits both an animation frame and
    /// this Promise, so a `setTimeout(fetch, 250)` correctly defers idle
    /// until after the timer fires AND its scheduled fetch lands. The
    /// wrapper is only meaningful on documents created *after* it is
    /// installed; `Page.addScriptToEvaluateOnNewDocument` ensures that.
    async fn ensure_timer_script_injected(&self) -> Result<()> {
        let cdp = self.cdp.clone();
        let sid = self.session_id.clone();
        self.timer_script_injected
            .ensure(move || async move {
                let script = r#"(() => {
  if (window.__ferrousInstrumented) return;
  try {
    Object.defineProperty(window, '__ferrousInstrumented', { value: true });
  } catch (e) { return; }
  let pending = 0;
  let resolvers = [];
  Object.defineProperty(window, '__ferrousPending', { get: () => pending });
  window.__ferrousAwaitTimers = () =>
    pending === 0 ? Promise.resolve() : new Promise(r => resolvers.push(r));
  const drain = () => {
    if (pending === 0 && resolvers.length) {
      const r = resolvers; resolvers = [];
      for (const fn of r) { try { fn(); } catch (e) {} }
    }
  };
  const origST = window.setTimeout;
  const origCT = window.clearTimeout;
  // Expose the original (unwrapped) setTimeout so internal infrastructure
  // — like the composite NetworkIdle flush — can schedule timers without
  // bumping the user-visible pending counter.
  window.__ferrousRawSetTimeout = origST;
  const active = new Set();
  window.setTimeout = function(fn) {
    const delay = arguments[1];
    const args = Array.prototype.slice.call(arguments, 2);
    pending++;
    let id;
    const wrapped = function() {
      active.delete(id);
      pending--;
      try { if (typeof fn === 'function') fn.apply(this, args); }
      finally { drain(); }
    };
    id = origST(wrapped, delay);
    active.add(id);
    return id;
  };
  window.clearTimeout = function(id) {
    if (active.has(id)) { active.delete(id); pending--; drain(); }
    return origCT(id);
  };
})();"#;
                cdp.send_command_with_session(
                    &sid,
                    "Page.addScriptToEvaluateOnNewDocument".to_string(),
                    Some(json!({ "source": script })),
                )
                .await
                .map(|_| ())
            })
            .await?;
        Ok(())
    }

    // ─── P3: Locator entry point ──────────────────────────────────────────

    /// Create a [`Locator`] for the given CSS selector.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    ///
    /// page.locator("button#submit").click().await?;
    /// page.locator("input[name=q]").type_text("rust").await?;
    /// page.locator(".result").wait_for().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn locator(&self, selector: &str) -> Locator {
        Locator::new(selector, self.clone())
    }

    // ─── P2: goto with WaitUntil ─────────────────────────────────────────

    /// Navigate to a URL and wait for the specified condition.
    ///
    /// # Arguments
    ///
    /// * `url`        — The URL to navigate to
    /// * `wait_until` — When to consider navigation complete
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// page.goto("https://example.com", WaitUntil::DomContentLoaded).await?;
    /// page.goto("https://example.com", WaitUntil::NetworkIdle).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), fields(url = %url, wait_until = ?wait_until, session_id = %self.session_id))]
    pub async fn goto(&self, url: &str, wait_until: WaitUntil) -> Result<()> {
        const TIMEOUT_SECS: u64 = 30;
        let url_owned = url.to_string();
        // Capture session_id so the async block can own it
        let session_id = self.session_id.clone();

        let event_method = match wait_until {
            WaitUntil::DomContentLoaded => "Page.domContentEventFired",
            WaitUntil::Load | WaitUntil::NetworkIdle => "Page.loadEventFired",
        };

        // ── Subscribe BEFORE sending any command (race-condition fix) ─────────
        // Filter by BOTH method name AND session_id so concurrent pages never
        // receive each other's load events (multi-page isolation fix).
        let mut event_rx = self.cdp.subscribe_events();
        // ─────────────────────────────────────────────────────────────────────

        // First goto on this page also enables Page domain events. All
        // concurrent gotos on the same Page cooperate via the shared init
        // guard rather than each sending their own Page.enable.
        self.ensure_page_enabled().await?;
        if matches!(wait_until, WaitUntil::NetworkIdle) {
            self.ensure_network_enabled().await?;
            self.ensure_timer_script_injected().await?;
        }

        let response = self
            .send_command("Page.navigate".to_string(), Some(json!({ "url": url })))
            .await?;

        if let Some(error_text) = response.get("errorText").and_then(|v| v.as_str()) {
            return Err(BrowserError::navigation_failed(&url_owned, error_text));
        }

        let cdp_for_flush = self.cdp.clone();

        // Subscribe to the disconnect signal *before* entering the wait
        // loop so we never miss a transition. The wait paths below select
        // on `disconnect_rx.changed()` so a dropped Browser surfaces as a
        // websocket error within ms rather than waiting out the 30 s
        // outer timeout. (RecvError::Closed on the event broadcast isn't
        // sufficient — Pages keep CDPClient alive, so the broadcast
        // sender doesn't drop just because Chrome exited.)
        let mut disconnect_rx = self.cdp.disconnected();
        if *disconnect_rx.borrow_and_update() {
            return Err(BrowserError::websocket(
                "goto",
                "CDP connection already closed",
            ));
        }

        let wait_result = timeout(Duration::from_secs(TIMEOUT_SECS), async {
            match wait_until {
                WaitUntil::NetworkIdle => {
                    use std::collections::HashSet;
                    // Composite signal: load fired ∧ in-flight==0 ∧
                    // (microtask queue drained + 1 rAF) ∧ in-flight still 0.
                    let mut in_flight: HashSet<String> = HashSet::new();
                    let mut load_fired = false;
                    let sid = session_id.clone();

                    let update = |msg: &crate::cdp::CDPMessage,
                                  load_fired: &mut bool,
                                  in_flight: &mut HashSet<String>| {
                        match msg.method.as_deref() {
                            Some("Network.requestWillBeSent") => {
                                let Some(params) = msg.params.as_ref() else {
                                    return;
                                };
                                // EventSource streams open a regular HTTP
                                // request that never receives a
                                // `Network.loadingFinished` until the
                                // connection closes. Counting them here
                                // would pin the in-flight counter for the
                                // page's lifetime and hang every
                                // `goto(NetworkIdle)` to the outer 30 s
                                // timeout. Skip them — users who want to
                                // wait for SSE traffic specifically can
                                // use `wait_for_function` on their own
                                // signal. WebSocket needs no such filter;
                                // Chrome emits `Network.webSocketCreated`
                                // for it, not `requestWillBeSent`.
                                let rtype = params
                                    .get("type")
                                    .and_then(|v| v.as_str());
                                if rtype == Some("EventSource") {
                                    return;
                                }
                                if let Some(id) = params
                                    .get("requestId")
                                    .and_then(|v| v.as_str())
                                {
                                    in_flight.insert(id.to_string());
                                }
                            }
                            Some("Network.loadingFinished")
                            | Some("Network.loadingFailed") => {
                                if let Some(id) = msg
                                    .params
                                    .as_ref()
                                    .and_then(|p| p.get("requestId"))
                                    .and_then(|v| v.as_str())
                                {
                                    in_flight.remove(id);
                                }
                            }
                            Some("Page.loadEventFired") => {
                                *load_fired = true;
                            }
                            _ => {}
                        }
                    };

                    'outer: loop {
                        // Drain events until both conditions hold.
                        while !(load_fired && in_flight.is_empty()) {
                            tokio::select! {
                                _ = disconnect_rx.changed() => {
                                    return Err(BrowserError::websocket(
                                        "goto/NetworkIdle composite wait",
                                        "CDP connection closed",
                                    ));
                                }
                                recv = event_rx.recv() => {
                                    match recv {
                                        Ok(msg)
                                            if msg.session_id.as_deref() == Some(&sid) =>
                                        {
                                            update(&msg, &mut load_fired, &mut in_flight);
                                        }
                                        Ok(_) => {}
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                            return Err(BrowserError::websocket(
                                                "goto/NetworkIdle composite wait",
                                                "CDP event stream closed",
                                            ));
                                        }
                                    }
                                }
                            }
                        }

                        // Force one rAF tick (which also flushes microtasks
                        // queued before it). Listen for new Network events
                        // concurrently — if anything appears during the flush,
                        // loop back rather than declaring idle.
                        // rAF flush + drain pending timers (instrumented by
                        // the document_start wrapper). If the wrapper isn't
                        // installed (e.g. injection failed) we fall back to
                        // a single rAF, matching C5 behaviour.
                        let flush_fut = cdp_for_flush.send_command_with_session(
                            &sid,
                            "Runtime.evaluate".to_string(),
                            Some(json!({
                                "expression": r#"(async () => {
    // Race rAF against a raw setTimeout(50). In headless Chrome a
    // backgrounded tab's rAF callbacks can be throttled or paused
    // entirely — without the fallback this Promise would never resolve.
    // We use the unwrapped setTimeout (__ferrousRawSetTimeout) so the
    // fallback doesn't bump the pending-timer counter.
    const rawST = window.__ferrousRawSetTimeout || window.setTimeout;
    await new Promise(r => {
        let done = false;
        const once = () => { if (!done) { done = true; r(); } };
        requestAnimationFrame(once);
        rawST.call(window, once, 50);
    });
    if (typeof window.__ferrousAwaitTimers === 'function') {
        while (window.__ferrousPending > 0) {
            await window.__ferrousAwaitTimers();
            await new Promise(r => {
                let done = false;
                const once = () => { if (!done) { done = true; r(); } };
                requestAnimationFrame(once);
                rawST.call(window, once, 50);
            });
        }
    }
    return true;
})()"#,
                                "awaitPromise": true,
                                "returnByValue": true,
                            })),
                        );
                        tokio::pin!(flush_fut);

                        loop {
                            tokio::select! {
                                _ = disconnect_rx.changed() => {
                                    return Err(BrowserError::websocket(
                                        "goto/NetworkIdle composite flush",
                                        "CDP connection closed",
                                    ));
                                }
                                _ = &mut flush_fut => {
                                    if load_fired && in_flight.is_empty() {
                                        return Ok::<(), BrowserError>(());
                                    }
                                    continue 'outer;
                                }
                                recv = event_rx.recv() => {
                                    match recv {
                                        Ok(msg)
                                            if msg.session_id.as_deref() == Some(&sid) =>
                                        {
                                            update(&msg, &mut load_fired, &mut in_flight);
                                        }
                                        Ok(_) => {}
                                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                            return Err(BrowserError::websocket(
                                                "goto/NetworkIdle composite flush",
                                                "CDP event stream closed",
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                _ => loop {
                    tokio::select! {
                        _ = disconnect_rx.changed() => {
                            return Err(BrowserError::websocket(
                                "goto wait",
                                "CDP connection closed",
                            ));
                        }
                        recv = event_rx.recv() => {
                            match recv {
                                Ok(msg)
                                    if msg.method.as_deref() == Some(event_method)
                                        && msg.session_id.as_deref() == Some(&session_id) =>
                                {
                                    return Ok(());
                                }
                                Ok(_) => {} // wrong session or wrong event
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                    return Ok(()); // assume fired
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    return Err(BrowserError::websocket(
                                        "goto wait",
                                        "CDP event stream closed",
                                    ));
                                }
                            }
                        }
                    }
                },
            }
        }.instrument(tracing::info_span!("await_navigation", event = event_method)))
        .await;

        wait_result.map_err(|_| {
            BrowserError::timeout(format!("navigating to '{}'", url_owned), TIMEOUT_SECS)
        })?
    }

    // ─── evaluate ─────────────────────────────────────────────────────────

    /// Evaluate a JavaScript expression and return a remote object handle.
    ///
    /// This is useful when you need a reference to a JavaScript object without
    /// serializing it back to Rust. The returned handle is valid only for this
    /// session and should be disposed of when no longer needed.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch_chrome(None).await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// // Get a remote reference to an object
    /// let handle = page.evaluate_handle("document.body").await?;
    /// println!("Remote object handle: {}", handle);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn evaluate_handle(&self, expression: &str) -> Result<String> {
        let result = self
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({
                    "expression": expression,
                    "returnByValue": false
                })),
            )
            .await?;

        if let Some(exc) = result.get("exceptionDetails") {
            let msg = exc
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("unknown JS exception");
            return Err(BrowserError::command_failed("Runtime.evaluate", msg));
        }

        result
            .get("result")
            .and_then(|v| v.get("objectId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BrowserError::invalid_response(
                    "evaluate_handle()",
                    "missing result.objectId — may have evaluated to a primitive",
                )
            })
    }

    /// Evaluate a JavaScript expression in the page context and deserialize the
    /// result as `T`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch_chrome(None).await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let title: String = page.evaluate("document.title").await?;
    /// let count: u64 = page.evaluate("document.querySelectorAll('a').length").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self), fields(expression_len = expression.len()))]
    pub async fn evaluate<T: DeserializeOwned>(&self, expression: &str) -> Result<T> {
        let result = self
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({
                    "expression": expression,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .await?;

        if let Some(exc) = result.get("exceptionDetails") {
            let msg = exc
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("unknown JS exception");
            return Err(BrowserError::command_failed("Runtime.evaluate", msg));
        }

        let value = result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null);

        serde_json::from_value(value)
            .map_err(|e| BrowserError::invalid_response("evaluate()", e.to_string()))
    }

    // ─── Wait helpers ─────────────────────────────────────────────────────

    /// Wait for an element matching `selector` to appear in the DOM.
    ///
    /// Uses a 30-second timeout.
    pub async fn wait_for_selector(&self, selector: &str) -> Result<()> {
        self.wait_for_selector_with_timeout(selector, Duration::from_secs(30))
            .await
    }

    /// Wait for an element matching `selector` with a custom timeout.
    ///
    /// Implementation note: we push the *entire* wait into Chrome with a
    /// MutationObserver-backed Promise and use `Runtime.evaluate`'s
    /// `awaitPromise: true` so Chrome holds the response until the element
    /// appears (or the timer fires). Net result is one CDP round-trip per
    /// call and a reaction latency bounded by the DOM mutation that
    /// inserted the element, not by a polling interval.
    pub async fn wait_for_selector_with_timeout(
        &self,
        selector: &str,
        dur: Duration,
    ) -> Result<()> {
        let timeout_ms = dur.as_millis() as u64;
        // The selector is interpolated into a JS string literal, so escape
        // anything that would break out of it. serde_json::to_string gives
        // us a properly-quoted JS string for free.
        let selector_lit = serde_json::to_string(selector).expect("selector is valid utf-8");

        let expr = format!(
            r#"new Promise((resolve) => {{
                const sel = {selector_lit};
                if (document.querySelector(sel)) {{ resolve(true); return; }}
                const observer = new MutationObserver(() => {{
                    if (document.querySelector(sel)) {{
                        observer.disconnect();
                        clearTimeout(timer);
                        resolve(true);
                    }}
                }});
                const timer = setTimeout(() => {{
                    observer.disconnect();
                    resolve(false);
                }}, {timeout_ms});
                observer.observe(document, {{
                    childList: true, subtree: true, attributes: true
                }});
            }})"#
        );

        let result = self
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({
                    "expression": expr,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .await?;

        let appeared = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if appeared {
            Ok(())
        } else {
            Err(BrowserError::timeout(
                format!("waiting for selector '{selector}'"),
                dur.as_secs(),
            ))
        }
    }

    /// Wait until a JavaScript expression evaluates truthy.
    ///
    /// The predicate runs inside the page and re-checks on the next
    /// animation frame, with a raw `setTimeout` fallback when rAF is
    /// throttled or paused in a backgrounded page. Resolves to `Ok(())`
    /// on truthy or `BrowserError::Timeout` on expiry. JS exceptions from
    /// the predicate surface as `BrowserError::CommandFailed`.
    pub async fn wait_for_function(&self, expr: &str, dur: Duration) -> Result<()> {
        let timeout_ms = dur.as_millis() as u64;
        // Wrap the predicate body in an arrow so the result of `expr` is
        // returned even when it's a bare expression. We swallow user
        // exceptions and surface them via Promise.reject so the outer
        // awaitPromise:true exception path catches them uniformly.
        let js = format!(
            r#"new Promise((resolve, reject) => {{
                const start = performance.now();
                const timeoutMs = {timeout_ms};
                const predicate = () => ({expr});
                const rawST = window.__ferrousRawSetTimeout || window.setTimeout;
                const check = () => {{
                    let result;
                    try {{
                        result = predicate();
                    }} catch (e) {{
                        reject(e); return;
                    }}
                    if (result) {{ resolve(true); return; }}
                    if (performance.now() - start >= timeoutMs) {{
                        resolve(false); return;
                    }}
                    let done = false;
                    const again = () => {{
                        if (!done) {{
                            done = true;
                            check();
                        }}
                    }};
                    requestAnimationFrame(again);
                    rawST.call(window, again, Math.min(50, timeoutMs));
                }};
                check();
            }})"#
        );
        let result = self
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({
                    "expression": js,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .await?;
        if let Some(exc) = result.get("exceptionDetails") {
            let msg = exc
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("predicate threw");
            return Err(BrowserError::command_failed("wait_for_function", msg));
        }
        let truthy = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if truthy {
            Ok(())
        } else {
            Err(BrowserError::timeout(
                format!("wait_for_function('{expr}')"),
                dur.as_secs(),
            ))
        }
    }

    /// Wait for `selector` to match an actionable element (attached, not
    /// disabled, visible, non-zero box) then dispatch a click. Implemented
    /// as one in-page Promise so reaction latency is one microtask after
    /// the blocking mutation — no Rust-side polling.
    pub(crate) async fn click_selector_auto_wait(
        &self,
        selector: &str,
        dur: Duration,
    ) -> Result<()> {
        let timeout_ms = dur.as_millis() as u64;
        let sel_lit = serde_json::to_string(selector).expect("selector is valid utf-8");
        let js = format!(
            r#"new Promise((resolve) => {{
                const sel = {sel_lit};
                const timeoutMs = {timeout_ms};
                function pick() {{
                    const el = document.querySelector(sel);
                    if (!el || !el.isConnected) return null;
                    if (el.disabled || el.getAttribute('aria-disabled') === 'true') return null;
                    const style = getComputedStyle(el);
                    if (style.visibility === 'hidden' || style.display === 'none') return null;
                    if (parseFloat(style.opacity || '1') === 0) return null;
                    const r = el.getBoundingClientRect();
                    if (r.width === 0 || r.height === 0) return null;
                    return el;
                }}
                let settled = false;
                let mo = null;
                let timer = null;
                function settle(v) {{
                    if (settled) return;
                    settled = true;
                    if (mo) mo.disconnect();
                    if (timer !== null) clearTimeout(timer);
                    resolve(v);
                }}
                function tryAct() {{
                    const el = pick();
                    if (el) {{
                        el.click();
                        settle(true);
                    }}
                }}
                tryAct();
                if (settled) return;
                mo = new MutationObserver(() => tryAct());
                mo.observe(document, {{
                    childList: true, subtree: true, attributes: true
                }});
                timer = setTimeout(() => settle(false), timeoutMs);
            }})"#
        );
        let result = self
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({
                    "expression": js,
                    "returnByValue": true,
                    "awaitPromise": true,
                })),
            )
            .await?;
        let ok = result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(BrowserError::timeout(
                format!("click_auto('{selector}')"),
                dur.as_secs(),
            ))
        }
    }

    // ─── Interaction helpers (internal, also used by Locator) ─────────────

    /// Click an element matching the selector (internal implementation).
    pub(crate) async fn click_selector(&self, selector: &str) -> Result<()> {
        let expr = format!(
            "document.querySelector('{}').click()",
            escape_selector(selector),
        );
        self.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": expr })),
        )
        .await?;
        Ok(())
    }

    /// Type text into an element (internal implementation).
    pub(crate) async fn type_text_selector(&self, selector: &str, text: &str) -> Result<()> {
        let focus_expr = format!(
            "document.querySelector('{}').focus()",
            escape_selector(selector)
        );
        self.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": focus_expr })),
        )
        .await?;

        for ch in text.chars() {
            self.send_command(
                "Input.dispatchKeyEvent".to_string(),
                Some(json!({
                    "type": "char",
                    "text": ch.to_string(),
                })),
            )
            .await?;
        }
        Ok(())
    }

    // ─── Public raw-selector methods (legacy / power-user API) ────────────

    /// Click an element matching the CSS selector.
    ///
    /// Prefer [`Page::locator`] for new code.
    pub async fn click(&self, selector: &str) -> Result<()> {
        self.click_selector(selector).await
    }

    /// Type text into an input element matching the CSS selector.
    ///
    /// Prefer [`Page::locator`] for new code.
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<()> {
        self.type_text_selector(selector, text).await
    }

    // ─── Content / screenshot ────────────────────────────────────────────

    /// Get the full HTML content of the page.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let html = page.content().await?;
    /// println!("HTML: {}", html);
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self))]
    pub async fn content(&self) -> Result<String> {
        let result = self
            .send_command(
                "Runtime.evaluate".to_string(),
                Some(json!({ "expression": "document.documentElement.outerHTML" })),
            )
            .await?;

        result
            .get("result")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                BrowserError::invalid_response("content()", "missing result.value string")
            })
    }

    /// Take a screenshot of the page and return PNG bytes.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let png = page.screenshot().await?;
    /// std::fs::write("screenshot.png", png)?;
    /// # Ok(())
    /// # }
    /// ```
    #[tracing::instrument(level = "info", skip(self))]
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let result = self
            .send_command("Page.captureScreenshot".to_string(), None)
            .await?;

        let base64_data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BrowserError::invalid_response("screenshot()", "missing data field"))?;

        tracing::info_span!("base64_decode", b64_len = base64_data.len())
            .in_scope(|| base64_decode(base64_data))
    }

    // ─── Network interception ────────────────────────────────────────────

    /// Intercept network requests matching a pattern.
    ///
    /// Enables request interception and calls the callback for matching
    /// requests. The callback receives `(url, resource_type)` and returns
    /// `true` to abort the request.
    pub async fn intercept_requests<F>(&self, callback: F) -> Result<()>
    where
        F: Fn(&str, &str) -> bool + Send + 'static,
    {
        let _ = self.send_command("Network.enable".to_string(), None).await;
        let _ = self
            .send_command(
                "Network.setRequestInterception".to_string(),
                Some(json!({ "patterns": [{ "urlPattern": "*" }] })),
            )
            .await;

        // ── P1: Subscribe BEFORE the enable command fires events ─────────────
        let mut event_rx = self.cdp.subscribe_events();
        // ────────────────────────────────────────────────────────────────────

        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            while let Ok(msg) = event_rx.recv().await {
                // Only handle Network.requestIntercepted for this page's session
                if msg.method.as_deref() != Some("Network.requestIntercepted") {
                    continue;
                }
                if msg.session_id.as_deref() != Some(&session_id) {
                    continue;
                }
                if let Some(params) = msg.params {
                    let url = params
                        .get("request")
                        .and_then(|r| r.get("url"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("");
                    let resource_type = params
                        .get("request")
                        .and_then(|r| r.get("resourceType"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("");
                    let request_id = params
                        .get("requestId")
                        .and_then(|r| r.as_str())
                        .unwrap_or("");

                    let should_abort = callback(url, resource_type);

                    let cdp_method = if should_abort {
                        "Network.abortRequest"
                    } else {
                        "Network.continueInterceptedRequest"
                    };

                    let _ = cdp
                        .send_command_with_session(
                            &session_id,
                            cdp_method.to_string(),
                            Some(json!({ "requestId": request_id })),
                        )
                        .await;
                }
            }
        });

        Ok(())
    }

    // ─── Session persistence ────────────────────────────────────────────────

    /// Get all cookies from the page.
    ///
    /// Retrieves all cookies visible to the current page, including
    /// expired cookies if they are still in the cookie jar.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let cookies = page.cookies().await?;
    /// for cookie in cookies {
    ///     println!("{}={}", cookie.name, cookie.value);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cookies(&self) -> Result<Vec<Cookie>> {
        let result = self
            .send_command("Network.getCookies".to_string(), None)
            .await?;

        let cookies_array = result
            .get("cookies")
            .and_then(|v| v.as_array())
            .ok_or_else(|| BrowserError::invalid_response("cookies()", "missing cookies array"))?;

        let mut cookies = Vec::new();
        for cookie_val in cookies_array {
            if let Ok(cookie) = serde_json::from_value::<Cookie>(cookie_val.clone()) {
                cookies.push(cookie);
            }
        }

        Ok(cookies)
    }

    /// Set cookies for the page (session persistence).
    ///
    /// Sets one or more cookies that will be visible to JavaScript and HTTP requests.
    /// Typically called before navigation to pre-populate cookies for authentication.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, Cookie, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// let cookies = vec![Cookie {
    ///     name: "session_id".to_string(),
    ///     value: "abc123xyz".to_string(),
    ///     domain: Some("example.com".to_string()),
    ///     ..Default::default()
    /// }];
    /// page.set_cookies(&cookies).await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_cookies(&self, cookies: &[Cookie]) -> Result<()> {
        // Convert cookies to JSON array with proper formatting for CDP
        let cookie_params: Vec<Value> = cookies
            .iter()
            .map(|c| {
                let mut obj = json!({
                    "name": c.name,
                    "value": c.value,
                });
                if let Some(domain) = &c.domain {
                    obj["domain"] = json!(domain);
                }
                if let Some(path) = &c.path {
                    obj["path"] = json!(path);
                }
                if let Some(expires) = c.expires {
                    obj["expires"] = json!(expires);
                }
                if c.secure {
                    obj["secure"] = json!(true);
                }
                if c.http_only {
                    obj["httpOnly"] = json!(true);
                }
                if let Some(same_site) = &c.same_site {
                    obj["sameSite"] = json!(same_site);
                }
                obj
            })
            .collect();

        self.send_command(
            "Network.setCookies".to_string(),
            Some(json!({ "cookies": cookie_params })),
        )
        .await?;

        Ok(())
    }

    // ─── PDF Export ──────────────────────────────────────────────────────────

    /// Export the page as PDF and return the bytes.
    ///
    /// Converts the current page to PDF format. By default, includes all pages
    /// and uses A4 paper size in portrait mode.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let pdf = page.pdf().await?;
    /// std::fs::write("page.pdf", pdf)?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pdf(&self) -> Result<Vec<u8>> {
        self.pdf_with_options(None).await
    }

    /// Export the page as PDF with custom options.
    ///
    /// Allows control over paper size, margins, scale, landscape mode, and more.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # use serde_json::json;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let options = json!({
    ///     "landscape": true,
    ///     "scale": 1.5,
    ///     "paperWidth": 11.0,
    ///     "paperHeight": 8.5,
    /// });
    /// let pdf = page.pdf_with_options(Some(&options)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn pdf_with_options(&self, options: Option<&Value>) -> Result<Vec<u8>> {
        let mut params = json!({
            "landscape": false,
            "displayHeaderFooter": false,
            "scale": 1.0,
            "paperWidth": 8.5,
            "paperHeight": 11.0,
            "marginTop": 0.4,
            "marginBottom": 0.4,
            "marginLeft": 0.4,
            "marginRight": 0.4,
            "preferCSSPageSize": true,
            "transferMode": "ReturnAsBase64",
        });

        // Merge with provided options
        if let Some(opts) = options {
            if let Some(obj) = params.as_object_mut() {
                if let Some(opts_obj) = opts.as_object() {
                    for (key, value) in opts_obj.iter() {
                        obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        let result = self
            .send_command("Page.printToPDF".to_string(), Some(params))
            .await?;

        let base64_data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BrowserError::invalid_response("pdf()", "missing data field"))?;

        base64_decode(base64_data)
    }

    // ─── Internal ─────────────────────────────────────────────────────────

    /// Send a command to this page's session
    pub(crate) async fn send_command(
        &self,
        method: String,
        params: Option<Value>,
    ) -> Result<Value> {
        self.cdp
            .send_command_with_session(&self.session_id, method, params)
            .await
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

/// Escape single-quotes in a CSS selector used inside JS string literals.
fn escape_selector(s: &str) -> String {
    s.replace('\'', "\\'")
}

/// Decode base64 string to bytes
fn base64_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    engine.decode(s).map_err(|e| {
        BrowserError::invalid_response("screenshot()", format!("base64 decode failed: {e}"))
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_until_default() {
        let w: WaitUntil = Default::default();
        assert!(matches!(w, WaitUntil::Load));
    }

    #[test]
    fn test_escape_selector_plain() {
        assert_eq!(escape_selector("button#id"), "button#id");
    }

    #[test]
    fn test_escape_selector_quotes() {
        assert_eq!(escape_selector("input[name='q']"), "input[name=\\'q\\']");
    }
}
