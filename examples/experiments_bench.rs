// experiments_bench — measure three specific perf hypotheses in isolation.
//
//   A. goto(NetworkIdle) total time on a static data URL.
//        With today's 50 ms tokio-side polling cadence, this should sit
//        somewhere in [load + 500, load + 550] ms with ~25 ms σ.
//
//   B. "Wait for a JS condition to become true."
//        - B-baseline: Rust-side `evaluate` loop with 50 ms sleep.
//        - B-new:      page.wait_for_function (Promise + rAF in the page).
//        Both measure: time between the condition flipping and the wait
//        returning, in *page* time (`performance.now()`).
//
//   C. "Click a button that is disabled until t=200 ms."
//        - C-baseline: wait_for(selector) + Rust-side poll for !disabled
//                      + click. (What a user has to write today.)
//        - C-new:      page.locator(sel).click() with in-page actionability.
//        Both measure: time between the button becoming enabled and the
//        click handler firing, in page time.
//
// Each scenario prints median / p10 / p90 / mean / σ over ITERS iterations.
// A summary JSON line is appended at the end for external aggregation.
//
use ferrous_browser::{Browser, Page, WaitUntil};
use serde_json::json;
use std::error::Error;
use std::time::{Duration, Instant};

type AnyError = Box<dyn Error>;

const ITERS: usize = 50;
const WARMUP_ITERS: usize = 3;

// ─── stats helpers ───────────────────────────────────────────────────────────

fn pct(xs: &[f64], p: f64) -> f64 {
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = (((v.len() - 1) as f64) * p / 100.0).round() as usize;
    v[idx]
}
fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}
fn stddev(xs: &[f64]) -> f64 {
    let m = mean(xs);
    let var = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / xs.len() as f64;
    var.sqrt()
}

#[derive(Clone)]
struct Stats {
    name: String,
    median: f64,
    p10: f64,
    p90: f64,
    mean: f64,
    sigma: f64,
    n: usize,
}

fn stats(name: &str, xs: &[f64]) -> Stats {
    Stats {
        name: name.to_string(),
        median: pct(xs, 50.0),
        p10: pct(xs, 10.0),
        p90: pct(xs, 90.0),
        mean: mean(xs),
        sigma: stddev(xs),
        n: xs.len(),
    }
}

fn print_stats(s: &Stats) {
    println!(
        "{:42} n={:3}  median={:7.2}ms  p10={:7.2}ms  p90={:7.2}ms  mean={:7.2}ms  σ={:6.2}ms",
        s.name, s.n, s.median, s.p10, s.p90, s.mean, s.sigma,
    );
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

// ─── scenarios ───────────────────────────────────────────────────────────────

const A_ITERS: usize = 20;

/// Workload shapes for scenario A.  All produce the *same* NetworkIdle call
/// site; only the page's network behaviour changes.
fn a_workloads() -> [(&'static str, String); 4] {
    let static_ = "<!doctype html><html><body>scenario-a-static</body></html>".to_string();

    // One inline fetch that completes immediately (data: URLs resolve sync-ish).
    let single_wave = r#"<!doctype html><html><body><script>
fetch('data:text/plain,one');
</script></body></html>"#
        .to_string();

    // Two-deep dependent chain.
    let chained = r#"<!doctype html><html><body><script>
fetch('data:text/plain,one').then(() => fetch('data:text/plain,two'));
</script></body></html>"#
        .to_string();

    // Deferred fetch via setTimeout — the case our 500ms quiet window catches
    // but the composite signal (load + counter + microtask + rAF) does NOT
    // unless we add timer instrumentation.
    let deferred = r#"<!doctype html><html><body><script>
setTimeout(() => fetch('data:text/plain,deferred'), 250);
</script></body></html>"#
        .to_string();

    [
        ("A1-static       ", static_),
        ("A2-single-wave  ", single_wave),
        ("A3-chained      ", chained),
        ("A4-deferred-250 ", deferred),
    ]
}

async fn scenario_a_network_idle_matrix(page: &Page) -> Result<Vec<Stats>, AnyError> {
    let mut results = Vec::new();
    for (label, html) in a_workloads().iter() {
        let url = format!("data:text/html,{}", urlencode(html));
        for _ in 0..WARMUP_ITERS {
            page.goto(&url, WaitUntil::NetworkIdle).await?;
        }
        let mut xs = Vec::with_capacity(A_ITERS);
        for _ in 0..A_ITERS {
            let t = Instant::now();
            page.goto(&url, WaitUntil::NetworkIdle).await?;
            xs.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        results.push(stats(&format!("A: {label}NetworkIdle total"), &xs));
    }
    Ok(results)
}

// Cycle through delays so we sample the full 50ms poll alignment window.
const DELAY_CYCLE_MS: &[u64] = &[200, 210, 220, 230, 240];

fn scenario_b_html(delay_ms: u64) -> String {
    format!(
        r#"<!doctype html><html><body><script>
window.__condValue = false;
window.__condAt = null;
setTimeout(() => {{
    window.__condValue = true;
    window.__condAt = performance.now();
}}, {delay_ms});
</script></body></html>"#
    )
}

async fn scenario_b_baseline(page: &Page) -> Result<Stats, AnyError> {
    let mut xs = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!("data:text/html,{}", urlencode(&scenario_b_html(delay)));
        page.goto(&url, WaitUntil::Load).await?;
        // What a user has to write today: poll evaluate from Rust.
        loop {
            let v: bool = page.evaluate("!!window.__condValue").await?;
            if v {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let now: f64 = page.evaluate("performance.now()").await?;
        let cond_at: f64 = page.evaluate("window.__condAt").await?;
        xs.push(now - cond_at);
    }
    Ok(stats("B-baseline (Rust poll 50ms): gap", &xs))
}

async fn scenario_b_new(page: &Page) -> Result<Stats, AnyError> {
    let mut xs = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!("data:text/html,{}", urlencode(&scenario_b_html(delay)));
        page.goto(&url, WaitUntil::Load).await?;
        page.wait_for_function("!!window.__condValue", std::time::Duration::from_secs(10))
            .await?;
        let now: f64 = page.evaluate("performance.now()").await?;
        let cond_at: f64 = page.evaluate("window.__condAt").await?;
        xs.push(now - cond_at);
    }
    Ok(stats("B-new (wait_for_function): gap", &xs))
}

fn scenario_c_html(delay_ms: u64) -> String {
    format!(
        r#"<!doctype html><html><body>
<button id="btn" disabled>click me</button>
<script>
window.__enabledAt = null;
window.__clickedAt = null;
document.getElementById('btn').addEventListener('click', () => {{
    window.__clickedAt = performance.now();
}});
setTimeout(() => {{
    document.getElementById('btn').disabled = false;
    window.__enabledAt = performance.now();
}}, {delay_ms});
</script></body></html>"#
    )
}

async fn scenario_c_baseline(page: &Page) -> Result<Stats, AnyError> {
    let mut xs = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!("data:text/html,{}", urlencode(&scenario_c_html(delay)));
        page.goto(&url, WaitUntil::Load).await?;
        // Today's pattern:
        //   1. wait_for(selector) — but only checks presence; #btn exists from t=0.
        //   2. user must poll disabled state manually.
        page.locator("#btn").wait_for().await?;
        loop {
            let enabled: bool = page
                .evaluate("!document.getElementById('btn').disabled")
                .await?;
            if enabled {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        page.locator("#btn").click().await?;
        let clicked_at: f64 = page.evaluate("window.__clickedAt").await?;
        let enabled_at: f64 = page.evaluate("window.__enabledAt").await?;
        xs.push(clicked_at - enabled_at);
    }
    Ok(stats("C-baseline (wait+poll+click): gap", &xs))
}

async fn scenario_c_new(page: &Page) -> Result<Stats, AnyError> {
    // Auto-wait click: same call site as today, but click_auto() does the wait.
    let mut xs = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!("data:text/html,{}", urlencode(&scenario_c_html(delay)));
        page.goto(&url, WaitUntil::Load).await?;
        page.locator("#btn").click_auto().await?;
        let clicked_at: f64 = page.evaluate("window.__clickedAt").await?;
        let enabled_at: f64 = page.evaluate("window.__enabledAt").await?;
        xs.push(clicked_at - enabled_at);
    }
    Ok(stats("C-new (auto-wait click): gap", &xs))
}

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;
    // Warm the chrome-side caches so first scenario isn't penalised.
    page.goto("about:blank", WaitUntil::Load).await?;

    let label = std::env::var("LABEL").unwrap_or_else(|_| "unlabeled".into());
    println!("=== experiments_bench [{label}] ===");

    let a_results = scenario_a_network_idle_matrix(&page).await?;
    for s in &a_results {
        print_stats(s);
    }

    let b_base = scenario_b_baseline(&page).await?;
    print_stats(&b_base);

    let b_new = scenario_b_new(&page).await?;
    print_stats(&b_new);

    let c_base = scenario_c_baseline(&page).await?;
    print_stats(&c_base);

    let c_new = scenario_c_new(&page).await?;
    print_stats(&c_new);

    let a_json: serde_json::Map<String, serde_json::Value> = a_results
        .iter()
        .map(|s| (s.name.clone(), stats_to_json(s)))
        .collect();
    let summary = json!({
        "label": label,
        "iters": ITERS,
        "A_workloads":         serde_json::Value::Object(a_json),
        "B_baseline":          stats_to_json(&b_base),
        "B_new":               stats_to_json(&b_new),
        "C_baseline":          stats_to_json(&c_base),
        "C_new":               stats_to_json(&c_new),
    });
    println!("RESULTS_JSON {}", summary);

    Ok(())
}

fn stats_to_json(s: &Stats) -> serde_json::Value {
    json!({
        "median": s.median,
        "p10": s.p10,
        "p90": s.p90,
        "mean": s.mean,
        "sigma": s.sigma,
        "n": s.n,
    })
}
