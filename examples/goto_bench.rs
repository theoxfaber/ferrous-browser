//! Specifically exercises Page::goto with a warm page, the hot path for any
//! navigation-heavy workload (test runners, scrapers, monitors).
use ferrous_browser::{Browser, WaitUntil};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;

    // Warm-up: prime the connection / DNS / first-page-load costs.
    page.goto("about:blank", WaitUntil::Load).await?;

    const ROUNDS: usize = 20;
    let mut times = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let t = Instant::now();
        page.goto("about:blank", WaitUntil::Load).await?;
        times.push(t.elapsed());
    }
    times.sort();
    let total: std::time::Duration = times.iter().sum();
    let avg = total / times.len() as u32;
    let median = times[times.len() / 2];
    let p10 = times[times.len() / 10];
    println!("{ROUNDS} gotos to about:blank on a warm page:");
    println!("  p10={p10:?}  median={median:?}  avg={avg:?}  max={:?}", times.last().unwrap());
    Ok(())
}
