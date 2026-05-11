//! Element appears at a known moment (measured server-side in the page);
//! wait_for_selector returns shortly after; we report the gap.
//!
//! Old impl (poll every 100ms): gap = 0..100ms + 1 CDP RTT
//! New impl (MutationObserver):  gap ≈ 1 CDP RTT
use ferrous_browser::{Browser, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;

    const ROUNDS: usize = 20;
    let mut gaps_ms: Vec<f64> = Vec::with_capacity(ROUNDS);

    for r in 0..ROUNDS {
        // After goto returns, the page injects #target after 200ms and records
        // window.__injectedAt = performance.now() (monotonic, ms-precision).
        // After wait_for returns, we read it back.
        let html = "<html><body><script>\
            window.__injectedAt = null;\
            setTimeout(() => {\
              const d = document.createElement('div');\
              d.id = 'target';\
              document.body.appendChild(d);\
              window.__injectedAt = performance.now();\
            }, 200);\
            </script></body></html>";
        let data_url = format!("data:text/html,{}", urlencode(html));
        page.goto(&data_url, WaitUntil::Load).await?;

        page.locator("#target").wait_for().await?;
        let returned_at: f64 = page.evaluate("performance.now()").await?;
        let injected_at: f64 = page.evaluate("window.__injectedAt").await?;
        let gap = returned_at - injected_at;
        gaps_ms.push(gap);
        println!("  [{r}] injected_at={injected_at:.1}ms returned_at={returned_at:.1}ms  gap={gap:.1}ms");
    }

    gaps_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = gaps_ms[gaps_ms.len() / 2];
    let avg: f64 = gaps_ms.iter().sum::<f64>() / gaps_ms.len() as f64;
    let max = gaps_ms.last().copied().unwrap();
    println!("\nGap from element-insert to wait_for return:");
    println!("  median={median:.1}ms  avg={avg:.1}ms  max={max:.1}ms");
    Ok(())
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
