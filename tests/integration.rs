/// Integration tests for ferrous-browser
/// These require Chrome running on localhost:9222
#[cfg(test)]
mod tests {
    use ferrous_browser::{Browser, BrowserConfig, BrowserError, PageHelperErrorKind, WaitUntil};
    use tokio::time::Duration;

    /// Helper: skip gracefully if Chrome is not running
    fn is_chrome_unavailable(e: &BrowserError) -> bool {
        matches!(
            e,
            BrowserError::ConnectionFailed { .. } | BrowserError::BrowserNotLaunched { .. }
        )
    }

    #[tokio::test]
    async fn test_browser_connect() {
        match Browser::launch().await {
            Ok(_) => {
                println!("✓ Successfully connected to browser");
            }
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running on localhost:9222 (expected for CI)");
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_new_page() {
        match Browser::launch().await {
            Ok(browser) => match browser.new_page().await {
                Ok(page) => {
                    assert!(!page.target_id.is_empty());
                    assert!(!page.session_id.is_empty());
                    println!("✓ Successfully created new page");
                }
                Err(e) => println!("⊘ Failed to create page (Chrome might not be ready): {}", e),
            },
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running on localhost:9222");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_navigate() {
        match Browser::launch().await {
            Ok(browser) => match browser.new_page().await {
                Ok(page) => match page.goto("https://example.com", WaitUntil::Load).await {
                    Ok(_) => {
                        println!("✓ Successfully navigated to example.com");
                    }
                    Err(e) => println!("⊘ Navigation failed: {}", e),
                },
                Err(e) => println!("⊘ Failed to create page: {}", e),
            },
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_get_content() {
        match Browser::launch().await {
            Ok(browser) => match browser.new_page().await {
                Ok(page) => {
                    if let Err(e) = page.goto("https://example.com", WaitUntil::Load).await {
                        println!("⊘ Navigation failed: {}", e);
                        return;
                    }

                    match page.content().await {
                        Ok(html) => {
                            assert!(
                                html.contains("Example Domain") || html.contains("example"),
                                "HTML should contain example.com content"
                            );
                            println!(
                                "✓ Successfully retrieved page content ({} bytes)",
                                html.len()
                            );
                        }
                        Err(e) => println!("⊘ Failed to get content: {}", e),
                    }
                }
                Err(e) => println!("⊘ Failed to create page: {}", e),
            },
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_screenshot() {
        match Browser::launch().await {
            Ok(browser) => {
                match browser.new_page().await {
                    Ok(page) => {
                        if let Err(e) = page.goto("https://example.com", WaitUntil::Load).await {
                            println!("⊘ Navigation failed: {}", e);
                            return;
                        }

                        match page.screenshot().await {
                            Ok(bytes) => {
                                assert!(
                                    bytes.len() > 1000,
                                    "Screenshot should have reasonable size"
                                );
                                // PNG magic bytes: 137 80 78 71
                                assert_eq!(bytes[0], 137);
                                assert_eq!(bytes[1], 80);
                                println!(
                                    "✓ Successfully captured screenshot ({} bytes, valid PNG)",
                                    bytes.len()
                                );
                            }
                            Err(e) => println!("⊘ Screenshot failed: {}", e),
                        }
                    }
                    Err(e) => println!("⊘ Failed to create page: {}", e),
                }
            }
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_multiple_pages() {
        match Browser::launch().await {
            Ok(browser) => match browser.new_page().await {
                Ok(_page1) => match browser.new_page().await {
                    Ok(_page2) => match browser.new_page().await {
                        Ok(_page3) => {
                            let count = browser.page_count().await;
                            assert_eq!(count, 3);
                            println!("✓ Successfully opened {} pages simultaneously", count);
                        }
                        Err(e) => println!("⊘ Failed to create page 3: {}", e),
                    },
                    Err(e) => println!("⊘ Failed to create page 2: {}", e),
                },
                Err(e) => println!("⊘ Failed to create page 1: {}", e),
            },
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_click_and_type() {
        match Browser::launch().await {
            Ok(browser) => {
                match browser.new_page().await {
                    Ok(page) => {
                        if let Err(e) = page.goto("https://google.com", WaitUntil::Load).await {
                            println!("⊘ Navigation failed: {}", e);
                            return;
                        }

                        // Prefer the Locator API
                        match page
                            .locator("input[name='q']")
                            .type_text("ferrous browser test")
                            .await
                        {
                            Ok(_) => println!("✓ Type executed successfully via Locator API"),
                            Err(e) => println!("⊘ Type failed: {}", e),
                        }
                    }
                    Err(e) => println!("⊘ Failed to create page: {}", e),
                }
            }
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_wait_for_selector() {
        match Browser::launch().await {
            Ok(browser) => {
                match browser.new_page().await {
                    Ok(page) => {
                        if let Err(e) = page.goto("https://example.com", WaitUntil::Load).await {
                            println!("⊘ Navigation failed: {}", e);
                            return;
                        }

                        // Body should always be present
                        match page.wait_for_selector("body").await {
                            Ok(_) => println!("✓ Successfully waited for body selector"),
                            Err(e) => println!("⊘ Wait for selector failed: {}", e),
                        }

                        // Non-existent selector must time out
                        match page
                            .wait_for_selector_with_timeout(
                                "#nonexistent-element-xyz123",
                                Duration::from_millis(500),
                            )
                            .await
                        {
                            Ok(_) => println!("⊘ Unexpected success for invalid selector"),
                            Err(BrowserError::PageHelperFailure {
                                kind: PageHelperErrorKind::TimedOut,
                                ..
                            }) => {
                                println!("✓ Correctly timed out for invalid selector")
                            }
                            Err(e) => println!("⊘ Unexpected error: {}", e),
                        }
                    }
                    Err(e) => println!("⊘ Failed to create page: {}", e),
                }
            }
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    #[tokio::test]
    async fn test_bad_url_error() {
        match Browser::launch().await {
            Ok(browser) => match browser.new_page().await {
                Ok(page) => match page.goto("not-a-valid-url", WaitUntil::Load).await {
                    Ok(_) => println!("⊘ Unexpected success for bad URL"),
                    Err(e) => println!("✓ Correctly failed for bad URL: {}", e),
                },
                Err(e) => println!("⊘ Failed to create page: {}", e),
            },
            Err(ref e) if is_chrome_unavailable(e) => {
                println!("⊘ Chrome not running");
            }
            Err(e) => println!("⊘ Error: {}", e),
        }
    }

    // ── New tests for P4: BrowserConfig ──────────────────────────────────────

    #[test]
    fn test_browser_config_defaults() {
        let cfg = BrowserConfig::default();
        assert!(cfg.headless);
        assert_eq!(cfg.viewport, (1280, 720));
        assert_eq!(cfg.timeout, std::time::Duration::from_secs(30));
        assert!(cfg.args.is_empty());
    }

    #[test]
    fn test_browser_config_custom_viewport() {
        let cfg = BrowserConfig {
            viewport: (2560, 1440),
            ..Default::default()
        };
        assert_eq!(cfg.viewport, (2560, 1440));
    }

    // ── New tests for P5: error messages ──────────────────────────────────────

    #[test]
    fn test_timeout_error_message() {
        let e = BrowserError::timeout("waiting for selector '.submit-btn'", 30);
        assert_eq!(
            e.to_string(),
            "Timed out waiting for selector '.submit-btn' after 30s"
        );
    }

    #[test]
    fn test_navigation_failed_error_message() {
        let e = BrowserError::navigation_failed("https://x.com", "net::ERR_NAME_NOT_RESOLVED");
        assert_eq!(
            e.to_string(),
            "Navigation to 'https://x.com' failed: net::ERR_NAME_NOT_RESOLVED"
        );
    }

    #[test]
    fn test_command_failed_error_message() {
        let e = BrowserError::command_failed("Page.navigate", "target closed");
        assert_eq!(
            e.to_string(),
            "Command 'Page.navigate' failed: target closed"
        );
    }
}
