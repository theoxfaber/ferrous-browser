use anyhow::Result;
use headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption;
use headless_chrome::{Browser, LaunchOptions};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

const CHROME_PATH: &str =
    "/home/ken/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome";
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

fn launch_once() -> Result<Browser> {
    // headless_chrome's `args` field is `Vec<&'a OsStr>` whose lifetime is tied
    // to the LaunchOptions; static string literals satisfy that.
    let extra_args: Vec<&OsStr> = vec![
        OsStr::new("--disable-gpu"),
        OsStr::new("--disable-dev-shm-usage"),
    ];
    let options = LaunchOptions {
        headless: true,
        sandbox: false,
        path: Some(PathBuf::from(CHROME_PATH)),
        args: extra_args,
        ..Default::default()
    };
    Ok(Browser::new(options)?)
}

fn main() -> Result<()> {
    // 1. cold launch
    let mut cold = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        let b = launch_once()?;
        cold.push(t.elapsed().as_secs_f64() * 1000.0);
        drop(b);
        sleep(Duration::from_millis(500));
    }
    println!(
        "launch_chrome           median={:.1}ms  p10={:.1}ms  (n={})",
        median(cold.clone()),
        p10(cold.clone()),
        cold.len()
    );

    let browser = launch_once()?;

    // 2. new_page (new_tab in headless_chrome parlance)
    let mut np = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let tab = browser.new_tab()?;
        np.push(t.elapsed().as_secs_f64() * 1000.0);
        let _ = tab.close(false);
    }
    println!(
        "new_page                median={:.1}ms  p10={:.1}ms  (n={})",
        median(np.clone()),
        p10(np.clone()),
        ITERS
    );

    // 3. goto about:blank
    let tab = browser.new_tab()?;
    let mut gt = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        tab.navigate_to("about:blank")?.wait_until_navigated()?;
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
        let _ = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        ss.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "screenshot              median={:.1}ms  p10={:.1}ms  (n={})",
        median(ss.clone()),
        p10(ss.clone()),
        ITERS
    );

    // 5. evaluate document.title
    let mut ev = Vec::new();
    for _ in 0..ITERS {
        let t = Instant::now();
        let _ = tab.evaluate("document.title", false)?;
        ev.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    println!(
        "evaluate                median={:.2}ms p10={:.2}ms (n={})",
        median(ev.clone()),
        p10(ev.clone()),
        ITERS
    );

    // 6. wait_for_selector reaction gap.
    // headless_chrome ships `wait_for_element`, which internally polls. We use
    // the canonical user API directly — no manual sleep loop required.
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
        tab.navigate_to(&data_url)?.wait_until_navigated()?;
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
        gaps.push(returned_at - injected_at);
    }
    println!(
        "wait_for_selector gap   median={:.2}ms p10={:.2}ms (n={}) [wait_for_element built-in]",
        median(gaps.clone()),
        p10(gaps.clone()),
        ITERS
    );

    Ok(())
}
