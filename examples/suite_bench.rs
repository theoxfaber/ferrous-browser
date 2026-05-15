#[allow(dead_code)]
#[path = "parity_bench.rs"]
mod parity_bench;
#[allow(dead_code)]
#[path = "realistic_bench.rs"]
mod realistic_bench;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("== ferrous-browser parity_bench ==");
    parity_bench::run().await?;

    println!();
    println!("== ferrous-browser realistic_bench ==");
    realistic_bench::run().await?;

    Ok(())
}
