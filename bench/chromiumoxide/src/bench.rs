use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::{Page, ScreenshotParams};
use futures::StreamExt;
use serde_json::json;
use std::time::{Duration, Instant};

const ITERS: usize = 20;
const DELAY_CYCLE_MS: &[u64] = &[200, 210, 220, 230, 240];

#[derive(Clone)]
struct Stats {
    median: f64,
    p10: f64,
    n: usize,
    note: Option<&'static str>,
}

fn chrome_path() -> String {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let home = std::env::var("HOME").expect("HOME must be set or CHROME_PATH must be provided");
    format!("{home}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome")
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

fn stats_with_note(xs: Vec<f64>, note: &'static str) -> Stats {
    Stats {
        median: median(xs.clone()),
        p10: p10(xs),
        n: ITERS,
        note: Some(note),
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

fn print_na(name: &str, note: &str) {
    println!("{name:24} n/a [{note}]");
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

async fn launch_once() -> Result<Browser, Box<dyn std::error::Error>> {
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path())
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .build()?;
    let (browser, mut handler) = Browser::launch(config).await?;
    tokio::spawn(async move { while handler.next().await.is_some() {} });
    Ok(browser)
}

async fn poll_until_true(page: &Page, expr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let timeout = Instant::now() + Duration::from_secs(10);
    loop {
        let ready: bool = page.evaluate(expr).await?.into_value().unwrap_or(false);
        if ready {
            return Ok(());
        }
        if Instant::now() >= timeout {
            return Err(format!("timed out waiting for expression: {expr}").into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cold = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let browser = launch_once().await?;
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

    let browser = launch_once().await?;

    let mut np = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let page = browser.new_page("about:blank").await?;
        np.push(t.elapsed().as_secs_f64() * 1000.0);
        let _ = page.close().await;
    }
    let new_page = stats(np);
    print_stats("new_page", &new_page);

    let page = browser.new_page("about:blank").await?;
    let mut gt = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        page.goto("about:blank")
            .await?
            .wait_for_navigation()
            .await?;
        gt.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let goto_about_blank = stats(gt);
    print_stats("goto_about_blank", &goto_about_blank);

    let mut ss = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await?;
        ss.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let screenshot = stats(ss);
    print_stats("screenshot", &screenshot);

    let mut ev = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _: String = page
            .evaluate("document.title")
            .await?
            .into_value()
            .unwrap_or_default();
        ev.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let evaluate = stats(ev);
    print_stats("evaluate", &evaluate);

    let selector_url = format!("data:text/html,{}", urlencode(&selector_gap_html()));
    let mut selector_gaps = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        page.goto(&selector_url)
            .await?
            .wait_for_navigation()
            .await?;
        let timeout = Instant::now() + Duration::from_secs(5);
        loop {
            if page.find_element("#target").await.is_ok() {
                break;
            }
            if Instant::now() > timeout {
                return Err("timed out waiting for #target".into());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let returned_at: f64 = page
            .evaluate("performance.now()")
            .await?
            .into_value()
            .unwrap_or(0.0);
        let injected_at: f64 = page
            .evaluate("window.__injectedAt")
            .await?
            .into_value()
            .unwrap_or(0.0);
        selector_gaps.push(returned_at - injected_at);
    }
    let wait_for_selector_gap = stats_with_note(selector_gaps, "manual poll @50ms");
    print_stats("wait_for_selector_gap", &wait_for_selector_gap);

    print_na("networkidle_static", "no first-class NetworkIdle wait");
    print_na(
        "networkidle_deferred_250",
        "no first-class NetworkIdle wait",
    );

    let mut wait_for_function_gaps = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!(
            "data:text/html,{}",
            urlencode(&wait_for_function_html(delay))
        );
        page.goto(&url).await?.wait_for_navigation().await?;
        poll_until_true(&page, "!!window.__condValue").await?;
        let returned_at: f64 = page
            .evaluate("performance.now()")
            .await?
            .into_value()
            .unwrap_or(0.0);
        let cond_at: f64 = page
            .evaluate("window.__condAt")
            .await?
            .into_value()
            .unwrap_or(0.0);
        wait_for_function_gaps.push(returned_at - cond_at);
    }
    let wait_for_function_gap = stats_with_note(wait_for_function_gaps, "manual poll @50ms");
    print_stats("wait_for_function_gap", &wait_for_function_gap);

    let mut click_when_enabled_gaps = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!(
            "data:text/html,{}",
            urlencode(&click_when_enabled_html(delay))
        );
        page.goto(&url).await?.wait_for_navigation().await?;
        poll_until_true(&page, "!document.getElementById('btn').disabled").await?;
        let button = page.find_element("#btn").await?;
        button.click().await?;
        let clicked_at: f64 = page
            .evaluate("window.__clickedAt")
            .await?
            .into_value()
            .unwrap_or(0.0);
        let enabled_at: f64 = page
            .evaluate("window.__enabledAt")
            .await?
            .into_value()
            .unwrap_or(0.0);
        click_when_enabled_gaps.push(clicked_at - enabled_at);
    }
    let click_when_enabled_gap = stats_with_note(click_when_enabled_gaps, "manual poll @50ms");
    print_stats("click_when_enabled_gap", &click_when_enabled_gap);

    println!(
        "RESULTS_JSON {}",
        json!({
            "library": "chromiumoxide",
            "metrics": {
                "launch_chrome": stats_to_json(&launch),
                "new_page": stats_to_json(&new_page),
                "goto_about_blank": stats_to_json(&goto_about_blank),
                "screenshot": stats_to_json(&screenshot),
                "evaluate": stats_to_json(&evaluate),
                "wait_for_selector_gap": stats_to_json(&wait_for_selector_gap),
                "networkidle_static": serde_json::Value::Null,
                "networkidle_deferred_250": serde_json::Value::Null,
                "wait_for_function_gap": stats_to_json(&wait_for_function_gap),
                "click_when_enabled_gap": stats_to_json(&click_when_enabled_gap),
            }
        })
    );

    Ok(())
}
