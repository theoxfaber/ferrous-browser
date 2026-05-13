// Verifies that the composite NetworkIdle signal correctly observes every
// fetch in each workload — i.e. that the in-page fetch counter has reached
// its expected value at the moment goto(NetworkIdle) returns. If the
// in-page counter is below expected, the signal fired early (false-positive
// idle) and the workload "got away" with unobserved work.

use ferrous_browser::{Browser, WaitUntil};

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
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;
    page.goto("about:blank", WaitUntil::Load).await?;

    let cases = [
        (
            "A1-static",
            0_u64,
            r#"<!doctype html><html><body>x</body></html>"#.to_string(),
        ),
        (
            "A2-single-wave",
            1,
            r#"<!doctype html><html><body><script>
window.__fetchedCount = 0;
fetch('data:text/plain,one').then(() => { window.__fetchedCount++; });
</script></body></html>"#
                .to_string(),
        ),
        (
            "A3-chained",
            2,
            r#"<!doctype html><html><body><script>
window.__fetchedCount = 0;
fetch('data:text/plain,one').then(() => {
    window.__fetchedCount++;
    return fetch('data:text/plain,two');
}).then(() => { window.__fetchedCount++; });
</script></body></html>"#
                .to_string(),
        ),
        (
            "A4-deferred-250",
            1,
            r#"<!doctype html><html><body><script>
window.__fetchedCount = 0;
setTimeout(() => {
    fetch('data:text/plain,deferred').then(() => { window.__fetchedCount++; });
}, 250);
</script></body></html>"#
                .to_string(),
        ),
    ];

    println!(
        "{:<18}  {:<8}  {:<8}  {:<8}",
        "workload", "expected", "observed", "verdict"
    );
    for (label, expected, html) in &cases {
        let url = format!("data:text/html,{}", urlencode(html));
        let t = std::time::Instant::now();
        page.goto(&url, WaitUntil::NetworkIdle).await?;
        let elapsed_ms = t.elapsed().as_secs_f64() * 1000.0;
        let observed: u64 = if *expected == 0 {
            0
        } else {
            page.evaluate("window.__fetchedCount").await?
        };
        let verdict = if observed >= *expected {
            "✓ complete"
        } else {
            "✗ early-return"
        };
        println!("{label:<18}  {expected:<8}  {observed:<8}  {verdict}    [{elapsed_ms:.1} ms]");
    }

    Ok(())
}
