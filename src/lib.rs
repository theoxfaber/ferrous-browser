#![deny(missing_docs)]

//! # ferrous-browser
//!
//! Full browser automation in Rust. No Node.js. No compromises. Single binary.
//!
//! ferrous-browser is a native Rust library for Chrome DevTools Protocol (CDP)
//! communication, enabling full browser automation without Node.js.
//!
//! ## Quick Start
//!
//! ```no_run
//! use ferrous_browser::{Browser, BrowserConfig, WaitUntil};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Launch Chrome with defaults (headless, 1280×720, 30 s timeout)
//!     let browser = Browser::launch_chrome(None).await?;
//!
//!     // Create a new page/tab
//!     let page = browser.new_page().await?;
//!
//!     // Navigate to a website
//!     page.goto("https://example.com", WaitUntil::Load).await?;
//!
//!     // Get the HTML content
//!     let html = page.content().await?;
//!     println!("Page HTML: {}", html);
//!
//!     // Take a screenshot
//!     let png_bytes = page.screenshot().await?;
//!     std::fs::write("screenshot.png", png_bytes)?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Features
//!
//! - **Zero Node.js dependency** — Pure Rust implementation using tokio
//! - **Single binary deployment** — No external runtime required
//! - **Async-first API** — Built on tokio for high performance
//! - **Type-safe** — Rust's type system catches errors at compile time
//! - **Direct CDP access** — Full control over Chrome's DevTools Protocol
//! - **Locator API** — Playwright-style ergonomic element selectors
//!
//! ## Locator API
//!
//! ```no_run
//! use ferrous_browser::{Browser, WaitUntil};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let browser = Browser::launch_chrome(None).await?;
//!     let page = browser.new_page().await?;
//!     page.goto("https://example.com", WaitUntil::Load).await?;
//!
//!     // Locator API
//!     page.locator("button#submit").click().await?;
//!     page.locator("input[name=q]").type_text("hello").await?;
//!     page.locator(".result").wait_for().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## WaitUntil modes
//!
//! ```no_run
//! use ferrous_browser::{Browser, WaitUntil};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let browser = Browser::launch_chrome(None).await?;
//! let page = browser.new_page().await?;
//!
//! // DOM parsed, sub-resources may still load
//! page.goto("https://example.com", WaitUntil::DomContentLoaded).await?;
//!
//! // All resources loaded (default)
//! page.goto("https://example.com", WaitUntil::Load).await?;
//!
//! // No network requests for 500 ms (SPA-friendly)
//! page.goto("https://example.com", WaitUntil::NetworkIdle).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Architecture
//!
//! ferrous-browser is built on a layered architecture:
//!
//! - **CDPClient** — Low-level CDP message routing
//! - **Connection** — WebSocket lifecycle management
//! - **Browser** — High-level browser control API
//! - **Page** — High-level page/tab automation API
//! - **Locator** — Lazy element handle for ergonomic element interaction

/// Chrome DevTools Protocol types and client
pub mod cdp;

/// WebSocket connection management
pub mod connection;

/// Error types
pub mod error;

/// Browser automation API
pub mod browser;

/// Page automation API
pub mod page;

pub use browser::{Browser, BrowserConfig};
pub use error::{BrowserError, BrowserLaunchErrorKind, PageHelperErrorKind, ResultExt};
pub use page::{
    Cookie, Locator, LossyQuality, LossyQualityError, Page, ScreenshotEncoding, ScreenshotOptions,
    WaitUntil,
};

#[cfg(test)]
mod tests {
    use crate::error::{BrowserError, ResultExt};

    #[test]
    fn it_works() {
        // Verify library loads correctly
    }

    #[test]
    fn test_error_display_timeout() {
        let e = BrowserError::timeout("waiting for selector '.submit-btn'", 30);
        assert_eq!(
            e.to_string(),
            "Timed out waiting for selector '.submit-btn' after 30s"
        );
    }

    #[test]
    fn test_error_display_navigation_failed() {
        let e = BrowserError::navigation_failed("https://x.com", "net::ERR_NAME_NOT_RESOLVED");
        assert_eq!(
            e.to_string(),
            "Navigation to 'https://x.com' failed: net::ERR_NAME_NOT_RESOLVED"
        );
    }

    #[test]
    fn test_result_context() {
        let result: crate::error::Result<()> = Err(BrowserError::timeout("connecting", 5));
        let with_ctx = result.context("Browser::launch_chrome");
        assert!(with_ctx.is_err());
        let msg = with_ctx.unwrap_err().to_string();
        assert!(msg.contains("Browser::launch_chrome"));
    }
}
