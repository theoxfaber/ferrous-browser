// One-off diagnostic: subscribe to Page.lifecycleEvent and log every
// lifecycle name + timestamp during a goto on (a) a trivial data URL
// and (b) a data URL that triggers real fetch activity. Tells us when
// networkIdle / networkAlmostIdle fire relative to navigation start.

use ferrous_browser::{Browser, WaitUntil};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;
    page.goto("about:blank", WaitUntil::Load).await?;

    for (label, html) in [
        (
            "trivial",
            "<!doctype html><html><body>hi</body></html>".to_string(),
        ),
        (
            "with-fetches",
            r#"<!doctype html><html><body><script>
fetch('data:text/plain,one');
setTimeout(() => fetch('data:text/plain,two'), 50);
setTimeout(() => fetch('data:text/plain,three'), 150);
</script></body></html>"#
                .to_string(),
        ),
    ] {
        let url = format!("data:text/html,{}", urlencoding::encode(&html).into_owned());

        // Time the goto-Load (cheap reference) and then time how long until
        // lifecycle names show up by hand-rolling the wait.
        let t0 = Instant::now();
        page.goto(&url, WaitUntil::Load).await?;
        let load_ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!("\n=== {label}: goto(Load) took {load_ms:.1} ms ===");

        // Now goto with NetworkIdle (our changed implementation) so we get
        // the actual lifecycle-event-driven wait. Time it.
        let t1 = Instant::now();
        page.goto(&url, WaitUntil::NetworkIdle).await?;
        let ni_ms = t1.elapsed().as_secs_f64() * 1000.0;
        println!("=== {label}: goto(NetworkIdle) took {ni_ms:.1} ms ===");
    }

    Ok(())
}

mod urlencoding {
    pub fn encode(s: &str) -> std::borrow::Cow<'_, str> {
        let mut out = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        std::borrow::Cow::Owned(out)
    }
}
