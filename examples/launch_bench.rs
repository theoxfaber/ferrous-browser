//! Measure cold launch_chrome wall time. Run a few times to get a stable number.
use ferrous_browser::{Browser, BrowserConfig};
use std::time::Instant;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut times = vec![];
    for i in 0..5 {
        let t = Instant::now();
        let browser = Browser::launch_chrome(Some(bench_browser_config())).await?;
        let elapsed = t.elapsed();
        times.push(elapsed);
        println!("  [{i}] launch_chrome: {elapsed:?}");
        drop(browser);
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let total: std::time::Duration = times.iter().sum();
    let avg = total / times.len() as u32;
    let min = times.iter().min().unwrap();
    println!("  avg: {avg:?}   min: {min:?}");
    Ok(())
}
