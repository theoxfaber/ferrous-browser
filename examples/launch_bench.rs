//! Measure cold launch_chrome wall time. Run a few times to get a stable number.
use ferrous_browser::Browser;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut times = vec![];
    for i in 0..5 {
        let t = Instant::now();
        let browser = Browser::launch_chrome(None).await?;
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
