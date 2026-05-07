use ferrous_browser::Browser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("Ferrous Browser - CDP Connection Example");
    println!("========================================\n");

    println!("To test this example, start Chrome with remote debugging:");
    println!("  google-chrome --remote-debugging-port=9222\n");

    // Connect to Chrome (defaults to localhost:9222)
    match Browser::launch().await {
        Ok(browser) => {
            println!("✓ Connected to browser");

            // Create a new page
            match browser.new_page().await {
                Ok(page) => {
                    println!("✓ Created new page");

                    // Navigate to example.com
                    if let Err(e) = page.goto("https://example.com", ferrous_browser::WaitUntil::Load).await {
                        eprintln!("Navigation failed: {}", e);
                    } else {
                        println!("✓ Navigated to https://example.com");
                    }

                    // Get page content
                    match page.content().await {
                        Ok(html) => {
                            println!("✓ Retrieved page content ({} bytes)", html.len());
                            println!("\nFirst 200 chars of HTML:");
                            println!("{}", &html[..std::cmp::min(200, html.len())]);
                        }
                        Err(e) => eprintln!("Failed to get content: {}", e),
                    }

                    // Take screenshot
                    match page.screenshot().await {
                        Ok(bytes) => {
                            println!("\n✓ Screenshot captured ({} bytes)", bytes.len());
                        }
                        Err(e) => eprintln!("Screenshot failed: {}", e),
                    }

                    println!("\n✓ All operations completed successfully!");
                }
                Err(e) => eprintln!("Failed to create page: {}", e),
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to browser: {}", e);
            eprintln!("\nMake sure Chrome is running with:");
            eprintln!("  google-chrome --remote-debugging-port=9222");
        }
    }

    Ok(())
}
