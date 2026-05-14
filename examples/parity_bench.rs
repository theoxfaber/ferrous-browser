// Ferrous side of the parity bench.
//
// Run:
//   cargo run --release --example parity_bench
//
// The output is both human-readable and machine-readable. The final
// `RESULTS_JSON` line is consumed by `bench/run_matrix.ts`.
use ferrous_browser::{Browser, BrowserConfig, Page, WaitUntil};
use serde_json::json;
use std::time::{Duration, Instant};

const ITERS: usize = 20;
const WARMUP_ITERS: usize = 3;
const DELAY_CYCLE_MS: &[u64] = &[200, 210, 220, 230, 240];

#[derive(Clone)]
struct Stats {
    median: f64,
    p10: f64,
    n: usize,
    note: Option<&'static str>,
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn p10(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[(xs.len() as f64 * 0.1) as usize]
}

fn stats(xs: Vec<f64>) -> Stats {
    Stats {
        median: median(xs.clone()),
        p10: p10(xs),
        n: ITERS,
        note: None,
    }
}

fn print_stats(name: &str, s: &Stats) {
    match s.note {
        Some(note) => println!(
            "{name:24} median={:.2}ms p10={:.2}ms (n={}) [{note}]",
            s.median, s.p10, s.n
        ),
        None => println!(
            "{name:24} median={:.2}ms p10={:.2}ms (n={})",
            s.median, s.p10, s.n
        ),
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

fn bench_chrome_path() -> String {
    if let Ok(path) = std::env::var("CHROME_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let home = std::env::var("HOME").expect("HOME must be set or CHROME_PATH must be provided");
    format!("{home}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome")
}

fn bench_browser_config() -> BrowserConfig {
    BrowserConfig {
        chrome_path: Some(bench_chrome_path()),
        ..Default::default()
    }
}

fn selector_gap_html() -> String {
    "<html><body><script>\
        window.__injectedAt = null;\
        setTimeout(() => {\
          const d = document.createElement('div'); d.id = 'target';\
          document.body.appendChild(d);\
          window.__injectedAt = performance.now();\
        }, 200);\
        </script></body></html>"
        .to_string()
}

fn networkidle_static_html() -> String {
    "<!doctype html><html><body>networkidle-static</body></html>".to_string()
}

fn networkidle_deferred_html() -> String {
    r#"<!doctype html><html><body><script>
setTimeout(() => fetch('data:text/plain,deferred'), 250);
</script></body></html>"#
        .to_string()
}

fn wait_for_function_html(delay_ms: u64) -> String {
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

fn click_when_enabled_html(delay_ms: u64) -> String {
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

fn stats_to_json(s: &Stats) -> serde_json::Value {
    json!({
        "median": s.median,
        "p10": s.p10,
        "n": s.n,
        "note": s.note,
    })
}

async fn bench_networkidle(page: &Page, html: &str) -> Result<Stats, Box<dyn std::error::Error>> {
    let url = format!("data:text/html,{}", urlencode(html));
    for _ in 0..WARMUP_ITERS {
        page.goto(&url, WaitUntil::NetworkIdle).await?;
    }
    let mut xs = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        page.goto(&url, WaitUntil::NetworkIdle).await?;
        xs.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(stats(xs))
}

async fn bench_wait_for_function(page: &Page) -> Result<Stats, Box<dyn std::error::Error>> {
    let mut xs = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!(
            "data:text/html,{}",
            urlencode(&wait_for_function_html(delay))
        );
        page.goto(&url, WaitUntil::Load).await?;
        page.wait_for_function("!!window.__condValue", Duration::from_secs(10))
            .await?;
        let returned_at: f64 = page.evaluate("performance.now()").await?;
        let cond_at: f64 = page.evaluate("window.__condAt").await?;
        xs.push(returned_at - cond_at);
    }
    Ok(stats(xs))
}

async fn bench_click_when_enabled(page: &Page) -> Result<Stats, Box<dyn std::error::Error>> {
    let mut xs = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!(
            "data:text/html,{}",
            urlencode(&click_when_enabled_html(delay))
        );
        page.goto(&url, WaitUntil::Load).await?;
        page.locator("#btn").click_auto().await?;
        let clicked_at: f64 = page.evaluate("window.__clickedAt").await?;
        let enabled_at: f64 = page.evaluate("window.__enabledAt").await?;
        xs.push(clicked_at - enabled_at);
    }
    Ok(stats(xs))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = bench_browser_config();

    let mut cold = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let browser = Browser::launch_chrome(Some(config.clone())).await?;
        cold.push(t.elapsed().as_secs_f64() * 1000.0);
        drop(browser);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let launch = Stats {
        median: median(cold.clone()),
        p10: p10(cold),
        n: 5,
        note: None,
    };
    print_stats("launch_chrome", &launch);

    let browser = Browser::launch_chrome(Some(config)).await?;

    let mut np = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _page = browser.new_page().await?;
        np.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let new_page = stats(np);
    print_stats("new_page", &new_page);

    let page = browser.new_page().await?;
    page.goto("about:blank", WaitUntil::Load).await?;

    let mut gt = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        page.goto("about:blank", WaitUntil::Load).await?;
        gt.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let goto_about_blank = stats(gt);
    print_stats("goto_about_blank", &goto_about_blank);

    let mut ss = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = page.screenshot().await?;
        ss.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let screenshot = stats(ss);
    print_stats("screenshot", &screenshot);

    let mut ev = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _: String = page.evaluate("document.title").await?;
        ev.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let evaluate = stats(ev);
    print_stats("evaluate", &evaluate);

    let selector_url = format!("data:text/html,{}", urlencode(&selector_gap_html()));
    let mut selector_gaps = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        page.goto(&selector_url, WaitUntil::Load).await?;
        page.locator("#target").wait_for().await?;
        let returned_at: f64 = page.evaluate("performance.now()").await?;
        let injected_at: f64 = page.evaluate("window.__injectedAt").await?;
        selector_gaps.push(returned_at - injected_at);
    }
    let wait_for_selector_gap = stats(selector_gaps);
    print_stats("wait_for_selector_gap", &wait_for_selector_gap);

    let networkidle_static = bench_networkidle(&page, &networkidle_static_html()).await?;
    print_stats("networkidle_static", &networkidle_static);

    let networkidle_deferred_250 = bench_networkidle(&page, &networkidle_deferred_html()).await?;
    print_stats("networkidle_deferred_250", &networkidle_deferred_250);

    let wait_for_function_gap = bench_wait_for_function(&page).await?;
    print_stats("wait_for_function_gap", &wait_for_function_gap);

    let click_when_enabled_gap = bench_click_when_enabled(&page).await?;
    print_stats("click_when_enabled_gap", &click_when_enabled_gap);

    println!(
        "RESULTS_JSON {}",
        json!({
            "library": "ferrous-browser",
            "metrics": {
                "launch_chrome": stats_to_json(&launch),
                "new_page": stats_to_json(&new_page),
                "goto_about_blank": stats_to_json(&goto_about_blank),
                "screenshot": stats_to_json(&screenshot),
                "evaluate": stats_to_json(&evaluate),
                "wait_for_selector_gap": stats_to_json(&wait_for_selector_gap),
                "networkidle_static": stats_to_json(&networkidle_static),
                "networkidle_deferred_250": stats_to_json(&networkidle_deferred_250),
                "wait_for_function_gap": stats_to_json(&wait_for_function_gap),
                "click_when_enabled_gap": stats_to_json(&click_when_enabled_gap),
            }
        })
    );

    Ok(())
}
