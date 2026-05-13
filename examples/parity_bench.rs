// Ferrous side of the parity bench.
//
// Run:
//   cargo run --release --example parity_bench
//
// Methodology mirrors bench/puppeteer/bench.js and bench/playwright/bench.js
// so the numbers in the README compare like-for-like.
use ferrous_browser::{Browser, WaitUntil};
use std::time::{Duration, Instant};

const ITERS: usize = 20;

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}
fn p10(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[(xs.len() as f64 * 0.1) as usize]
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. cold launch_chrome × 5
    let mut cold = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let b = Browser::launch_chrome(None).await?;
        cold.push(t.elapsed().as_secs_f64() * 1000.0);
        drop(b);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    println!(
        "launch_chrome           median={:.1}ms  p10={:.1}ms  (n={})",
        median(cold.clone()),
        p10(cold.clone()),
        cold.len()
    );

    // Warm browser for all subsequent benches.
    let browser = Browser::launch_chrome(None).await?;

    // 2. new_page × ITERS
    let mut np = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let _p = browser.new_page().await?;
        np.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "new_page                median={:.1}ms  p10={:.1}ms  (n={})",
        median(np.clone()),
        p10(np.clone()),
        ITERS
    );

    // 3. goto about:blank × ITERS on a warm page
    let page = browser.new_page().await?;
    page.goto("about:blank", WaitUntil::Load).await?; // warmup
    let mut gt = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        page.goto("about:blank", WaitUntil::Load).await?;
        gt.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "goto about:blank        median={:.1}ms  p10={:.1}ms  (n={})",
        median(gt.clone()),
        p10(gt.clone()),
        ITERS
    );

    // 4. screenshot
    let mut ss = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = page.screenshot().await?;
        ss.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "screenshot              median={:.1}ms  p10={:.1}ms  (n={})",
        median(ss.clone()),
        p10(ss.clone()),
        ITERS
    );

    // 5. evaluate
    let mut ev = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let _: String = page.evaluate("document.title").await?;
        ev.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "evaluate                median={:.2}ms p10={:.2}ms (n={})",
        median(ev.clone()),
        p10(ev.clone()),
        ITERS
    );

    // 6. wait_for_selector reaction gap
    let html = "<html><body><script>\
        window.__injectedAt = null;\
        setTimeout(() => {\
          const d = document.createElement('div'); d.id = 'target';\
          document.body.appendChild(d);\
          window.__injectedAt = performance.now();\
        }, 200);\
        </script></body></html>";
    let data_url = format!("data:text/html,{}", urlencode(html));

    let mut gaps = Vec::new();
    for _ in 0..ITERS {
        page.goto(&data_url, WaitUntil::Load).await?;
        page.locator("#target").wait_for().await?;
        let returned_at: f64 = page.evaluate("performance.now()").await?;
        let injected_at: f64 = page.evaluate("window.__injectedAt").await?;
        gaps.push(returned_at - injected_at);
    }
    println!(
        "wait_for_selector gap   median={:.2}ms p10={:.2}ms (n={})",
        median(gaps.clone()),
        p10(gaps.clone()),
        ITERS
    );

    Ok(())
}
