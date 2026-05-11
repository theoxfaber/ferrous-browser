//! Profile driver. Runs the operations the README benchmarks measure
//! against a warm browser and writes a Chrome-trace-format JSON.
//!
//! Run:
//!     cargo run --release --example profile_run
//!
//! Open the produced `ferrous-trace.json` at https://ui.perfetto.dev.
//!
//! With Tracy instead:
//!     cargo run --release --features tracy --example profile_run
//! (and have the Tracy GUI listening; it auto-connects.)

use ferrous_browser::{Browser, WaitUntil};
use std::time::Instant;
use tracing_chrome::ChromeLayerBuilder;
use tracing_subscriber::{prelude::*, EnvFilter};

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (chrome_layer, _guard) = ChromeLayerBuilder::new()
        .file("ferrous-trace.json")
        .include_args(true)
        .build();

    let registry = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(chrome_layer);

    #[cfg(feature = "tracy")]
    let registry = registry.with(tracing_tracy::TracyLayer::default());

    registry.init();

    const ITERS: usize = 5;

    let total_start = Instant::now();
    println!("Launching Chrome…");
    let browser = Browser::launch_chrome(None).await?;
    println!("  launch_chrome took {:?}", total_start.elapsed());

    // Pre-create one page so the warm screenshot/evaluate benches don't include
    // creation time.
    let warm_page = browser.new_page().await?;
    warm_page.goto("https://example.com", WaitUntil::Load).await?;

    println!("\nbench: new_page (x{ITERS})");
    for i in 0..ITERS {
        let t = Instant::now();
        let _p = browser.new_page().await?;
        println!("  [{i}] {:?}", t.elapsed());
    }

    println!("\nbench: navigate + content (x{ITERS})");
    for i in 0..ITERS {
        let p = browser.new_page().await?;
        let t = Instant::now();
        p.goto("https://example.com", WaitUntil::Load).await?;
        let _ = p.content().await?;
        println!("  [{i}] {:?}", t.elapsed());
    }

    println!("\nbench: screenshot (x{ITERS})");
    for i in 0..ITERS {
        let t = Instant::now();
        let _ = warm_page.screenshot().await?;
        println!("  [{i}] {:?}", t.elapsed());
    }

    println!("\nbench: evaluate (x{ITERS})");
    for i in 0..ITERS {
        let t = Instant::now();
        let _: String = warm_page.evaluate("document.title").await?;
        println!("  [{i}] {:?}", t.elapsed());
    }

    println!("\nTotal wall: {:?}", total_start.elapsed());
    println!("Trace written to ferrous-trace.json. Open at https://ui.perfetto.dev");
    Ok(())
}
