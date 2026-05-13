use anyhow::Result;
use headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption;
use headless_chrome::{Browser, LaunchOptions, Tab};
use serde_json::json;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::thread::sleep;
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

fn chrome_path() -> PathBuf {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let home = std::env::var("HOME").expect("HOME must be set or CHROME_PATH must be provided");
    PathBuf::from(format!(
        "{home}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome"
    ))
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

fn launch_once() -> Result<Browser> {
    let extra_args: Vec<&OsStr> = vec![
        OsStr::new("--disable-gpu"),
        OsStr::new("--disable-dev-shm-usage"),
    ];
    let options = LaunchOptions {
        headless: true,
        sandbox: false,
        path: Some(chrome_path()),
        args: extra_args,
        ..Default::default()
    };
    Ok(Browser::new(options)?)
}

fn poll_until_true(tab: &Tab, expr: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let ready = tab
            .evaluate(expr, false)?
            .value
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for expression: {expr}");
        }
        sleep(Duration::from_millis(50));
    }
}

fn bench_networkidle(tab: &Tab, html: &str) -> Result<Stats> {
    let url = format!("data:text/html,{}", urlencode(html));
    for _ in 0..WARMUP_ITERS {
        tab.navigate_to(&url)?.wait_until_navigated()?;
    }
    let mut xs = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        tab.navigate_to(&url)?.wait_until_navigated()?;
        xs.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(stats(xs))
}

fn main() -> Result<()> {
    let mut cold = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let browser = launch_once()?;
        cold.push(t.elapsed().as_secs_f64() * 1000.0);
        drop(browser);
        sleep(Duration::from_millis(500));
    }
    let launch = Stats {
        median: median(cold.clone()),
        p10: p10(cold),
        n: 5,
        note: None,
    };
    print_stats("launch_chrome", &launch);

    let browser = launch_once()?;

    let mut np = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let tab = browser.new_tab()?;
        np.push(t.elapsed().as_secs_f64() * 1000.0);
        let _ = tab.close(false);
    }
    let new_page = stats(np);
    print_stats("new_page", &new_page);

    let tab = browser.new_tab()?;
    let mut gt = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        tab.navigate_to("about:blank")?.wait_until_navigated()?;
        gt.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let goto_about_blank = stats_with_note(gt, "wait_until_navigated uses networkAlmostIdle");
    print_stats("goto_about_blank", &goto_about_blank);

    let mut ss = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        ss.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let screenshot = stats(ss);
    print_stats("screenshot", &screenshot);

    let mut ev = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = tab.evaluate("document.title", false)?;
        ev.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let evaluate = stats(ev);
    print_stats("evaluate", &evaluate);

    let selector_url = format!("data:text/html,{}", urlencode(&selector_gap_html()));
    let mut selector_gaps = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        tab.navigate_to(&selector_url)?.wait_until_navigated()?;
        let _ = tab.wait_for_element("#target")?;
        let returned_at: f64 = tab
            .evaluate("performance.now()", false)?
            .value
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let injected_at: f64 = tab
            .evaluate("window.__injectedAt", false)?
            .value
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        selector_gaps.push(returned_at - injected_at);
    }
    let wait_for_selector_gap =
        stats_with_note(selector_gaps, "wait_for_element built-in polling");
    print_stats("wait_for_selector_gap", &wait_for_selector_gap);

    let networkidle_static = bench_networkidle(&tab, &networkidle_static_html())?;
    print_stats("networkidle_static", &networkidle_static);

    let networkidle_deferred_250 = bench_networkidle(&tab, &networkidle_deferred_html())?;
    print_stats("networkidle_deferred_250", &networkidle_deferred_250);

    let mut wait_for_function_gaps = Vec::with_capacity(ITERS);
    for i in 0..ITERS {
        let delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.len()];
        let url = format!(
            "data:text/html,{}",
            urlencode(&wait_for_function_html(delay))
        );
        tab.navigate_to(&url)?.wait_until_navigated()?;
        poll_until_true(&tab, "!!window.__condValue")?;
        let returned_at: f64 = tab
            .evaluate("performance.now()", false)?
            .value
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let cond_at: f64 = tab
            .evaluate("window.__condAt", false)?
            .value
            .and_then(|v| v.as_f64())
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
        tab.navigate_to(&url)?.wait_until_navigated()?;
        poll_until_true(&tab, "!document.getElementById('btn').disabled")?;
        tab.find_element("#btn")?.click()?;
        let clicked_at: f64 = tab
            .evaluate("window.__clickedAt", false)?
            .value
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let enabled_at: f64 = tab
            .evaluate("window.__enabledAt", false)?
            .value
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        click_when_enabled_gaps.push(clicked_at - enabled_at);
    }
    let click_when_enabled_gap = stats_with_note(click_when_enabled_gaps, "manual poll @50ms");
    print_stats("click_when_enabled_gap", &click_when_enabled_gap);

    println!(
        "RESULTS_JSON {}",
        json!({
            "library": "headless_chrome",
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
