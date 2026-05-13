use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::ScreenshotParams;
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use futures::StreamExt;
use std::time::{Duration, Instant};

const ITERS: usize = 20;

fn chrome_path() -> String {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        return p;
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
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

async fn launch_once() -> Result<Browser, Box<dyn std::error::Error>> {
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path())
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .build()?;
    let (browser, mut handler) = Browser::launch(config).await?;
    // chromiumoxide requires you to drive the handler in a background task.
    tokio::spawn(async move {
        while let Some(_) = handler.next().await {}
    });
    Ok(browser)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. cold launch
    let mut cold = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let b = launch_once().await?;
        cold.push(t.elapsed().as_secs_f64() * 1000.0);
        drop(b);
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    println!("launch_chrome           median={:.1}ms  p10={:.1}ms  (n={})",
        median(cold.clone()), p10(cold.clone()), cold.len());

    let browser = launch_once().await?;

    // 2. new_page
    let mut np = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let p = browser.new_page("about:blank").await?;
        np.push(t.elapsed().as_secs_f64() * 1000.0);
        let _ = p.close().await;
    }
    println!("new_page                median={:.1}ms  p10={:.1}ms  (n={})",
        median(np.clone()), p10(np.clone()), ITERS);

    // 3. goto about:blank
    let page = browser.new_page("about:blank").await?;
    let mut gt = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        page.goto("about:blank").await?.wait_for_navigation().await?;
        gt.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("goto about:blank        median={:.1}ms  p10={:.1}ms  (n={})",
        median(gt.clone()), p10(gt.clone()), ITERS);

    // 4. screenshot
    let mut ss = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = page.screenshot(ScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png).build()).await?;
        ss.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("screenshot              median={:.1}ms  p10={:.1}ms  (n={})",
        median(ss.clone()), p10(ss.clone()), ITERS);

    // 5. evaluate
    let mut ev = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let _: String = page.evaluate("document.title").await?
            .into_value().unwrap_or_default();
        ev.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!("evaluate                median={:.2}ms p10={:.2}ms (n={})",
        median(ev.clone()), p10(ev.clone()), ITERS);

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

    // 6. wait_for_selector reaction gap.
    // chromiumoxide doesn't ship a wait_for_selector. The canonical user
    // pattern (per its examples / community code) is a retry loop with sleep.
    // We use 50ms because it's a reasonable middle-ground; faster means more
    // CDP traffic, slower means worse reaction latency.
    let mut gaps = Vec::new();
    for _ in 0..ITERS {
        page.goto(&data_url).await?.wait_for_navigation().await?;
        // Manual wait-for-selector loop, as a chromiumoxide user would write it.
        let timeout = Instant::now() + Duration::from_secs(5);
        loop {
            if page.find_element("#target").await.is_ok() { break; }
            if Instant::now() > timeout {
                return Err("timed out waiting for #target".into());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let returned_at: f64 = page.evaluate("performance.now()").await?
            .into_value().unwrap_or(0.0);
        let injected_at: f64 = page.evaluate("window.__injectedAt").await?
            .into_value().unwrap_or(0.0);
        gaps.push(returned_at - injected_at);
    }
    println!("wait_for_selector gap   median={:.2}ms p10={:.2}ms (n={}) [manual poll @50ms]",
        median(gaps.clone()), p10(gaps.clone()), ITERS);

    Ok(())
}
