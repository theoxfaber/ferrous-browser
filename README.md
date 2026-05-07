# ferrous-browser

**Fast, async Rust browser automation via the Chrome DevTools Protocol — no Node.js required.**

[![Crates.io](https://img.shields.io/crates/v/ferrous-browser.svg)](https://crates.io/crates/ferrous-browser)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Build](https://img.shields.io/github/actions/workflow/status/theoxfaber/ferrous-browser/ci.yml?branch=main)](https://github.com/theoxfaber/ferrous-browser/actions)

---

## Why ferrous-browser?

Every Rust browser-automation library either wraps Node.js (slow, heavy) or is unmaintained. ferrous-browser is a pure-Rust, async-first CDP client with:

- **Zero Node.js** — pure Rust, ships as a single binary
- **Async-first** — built on Tokio; naturally integrates with any async Rust project
- **Correct multi-page isolation** — CDP session IDs are tracked; concurrent pages don't cross-contaminate events
- **Race-condition-free** — event handlers are registered *before* the commands that trigger them
- **Ergonomic API** — Playwright-inspired `locator()`, `evaluate()`, `WaitUntil`

---

## Installation

```toml
[dependencies]
ferrous-browser = "0.1"
tokio = { version = "1", features = ["full"] }
```

Requires **Google Chrome** or **Chromium** installed locally.

---

## Quick start

```rust
use ferrous_browser::{Browser, BrowserConfig, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Launch Chrome automatically (headless by default)
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;

    page.goto("https://example.com", WaitUntil::Load).await?;

    // Locator API
    let heading = page.locator("h1").inner_text().await?;
    println!("Heading: {heading}");

    // Raw JS evaluation
    let title: String = page.evaluate("document.title").await?;
    println!("Title: {title}");

    // Screenshot to file
    let png = page.screenshot().await?;
    std::fs::write("screenshot.png", png)?;

    Ok(())
}
```

---

## Navigation wait modes

```rust
// Wait for DOM parsed (fast, sub-resources still loading)
page.goto(url, WaitUntil::DomContentLoaded).await?;

// Wait for all resources loaded (default)
page.goto(url, WaitUntil::Load).await?;

// Wait until no network activity for 500 ms (best for SPAs)
page.goto(url, WaitUntil::NetworkIdle).await?;
```

---

## Locator API

```rust
let page = browser.new_page().await?;
page.goto("https://example.com", WaitUntil::Load).await?;

// Click
page.locator("button#submit").click().await?;

// Type
page.locator("input[name=q]").type_text("ferrous browser").await?;

// Wait until visible
page.locator(".result-list").wait_for().await?;

// Read text / attribute
let text = page.locator("h1").inner_text().await?;
let href = page.locator("a.main").get_attribute("href").await?;
```

---

## Evaluate JavaScript

```rust
let count: u64 = page.evaluate("document.querySelectorAll('a').length").await?;
let is_logged_in: bool = page.evaluate("!!document.cookie.includes('session')").await?;
let title: String = page.evaluate("document.title").await?;
```

---

## Browser configuration

```rust
use ferrous_browser::{Browser, BrowserConfig};
use std::time::Duration;

let config = BrowserConfig {
    headless: false,                        // visible window
    timeout: Duration::from_secs(60),       // startup timeout
    viewport: (1920, 1080),                 // window size
    args: vec!["--disable-extensions".to_string()],
};

let browser = Browser::launch_chrome(Some(config)).await?;
```

| Field | Default | Description |
|-------|---------|-------------|
| `headless` | `true` | Headless mode |
| `timeout` | 30 s | Chrome startup deadline |
| `viewport` | 1280×720 | Window size in logical pixels |
| `args` | `[]` | Extra Chrome CLI flags |

---

## Error handling

Every error carries structured context — no more "something went wrong":

```rust
use ferrous_browser::{BrowserError, ResultExt};

match page.goto("https://bad-url", WaitUntil::Load).await {
    Err(BrowserError::NavigationFailed { url, reason }) =>
        eprintln!("Navigation to {url} failed: {reason}"),
    Err(BrowserError::Timeout { operation, secs }) =>
        eprintln!("{operation} timed out after {secs}s"),
    Err(e) => eprintln!("Error: {e}"),
    Ok(_) => {}
}
```

`.context()` for chaining context onto any `Result`:

```rust
page.goto("https://example.com", WaitUntil::Load)
    .await
    .context("loading homepage")?;
```

---

## Benchmarks

Measured on macOS (Apple M-series), Chrome 147, localhost CDP, 100 Criterion samples.  
All operations use an already-running Chrome instance (no startup overhead).

| Operation | ferrous-browser (median) | Notes |
|-----------|--------------------------|-------|
| CDP connect | **~755 µs** | WebSocket handshake to local Chrome |
| New page/tab | **~756 µs** | `Target.createTarget` + `attachToTarget` |
| Navigate + content | **~462 µs** | Re-navigate cached page + `outerHTML` |
| Screenshot (PNG) | **~8–15 ms** | `Page.captureScreenshot` round-trip |
| JS evaluate | **~0.5–1 ms** | `Runtime.evaluate` + deserialize |

> **Note:** Navigate times reflect a warm browser navigating to a cached page. First-load times for uncached pages depend on network and page complexity (typically 200 ms–2 s).

---

## Real-world examples

### Web scraper

```rust
use ferrous_browser::{Browser, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;

    page.goto("https://news.ycombinator.com", WaitUntil::Load).await?;

    let title_count: u64 = page
        .evaluate("document.querySelectorAll('.titleline').length")
        .await?;
    println!("Found {title_count} stories");

    Ok(())
}
```

### End-to-end test

```rust
use ferrous_browser::{Browser, BrowserConfig, WaitUntil};

#[tokio::test]
async fn test_login_flow() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch_chrome(Some(BrowserConfig {
        headless: true,
        ..Default::default()
    })).await?;
    let page = browser.new_page().await?;

    page.goto("http://localhost:3000/login", WaitUntil::Load).await?;
    page.locator("input[name=email]").type_text("user@example.com").await?;
    page.locator("input[name=password]").type_text("secret").await?;
    page.locator("button[type=submit]").click().await?;
    page.locator(".dashboard").wait_for().await?;

    let url: String = page.evaluate("location.href").await?;
    assert!(url.contains("/dashboard"));
    Ok(())
}
```

### Screenshot utility

```rust
use ferrous_browser::{Browser, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::launch_chrome(None).await?;
    let page = browser.new_page().await?;
    page.goto("https://example.com", WaitUntil::NetworkIdle).await?;
    let png = page.screenshot().await?;
    std::fs::write("out.png", png)?;
    println!("Saved out.png");
    Ok(())
}
```

---

## Comparison

| | ferrous-browser | chromiumoxide | headless_chrome |
|---|---|---|---|
| Language | Rust | Rust | Rust |
| Node.js required | ❌ | ❌ | ❌ |
| Actively maintained | ✅ | ⚠️ stale | ❌ archived |
| Multi-page session isolation | ✅ | ✅ | ⚠️ |
| `page.evaluate::<T>()` | ✅ | ✅ | ✅ |
| Locator API | ✅ | ❌ | ❌ |
| `WaitUntil::NetworkIdle` | ✅ | ❌ | ❌ |
| Structured errors | ✅ | ⚠️ | ⚠️ |

---

## Roadmap

- [ ] `page.set_cookies()` / `page.cookies()` — session persistence
- [ ] `page.pdf()` — PDF export
- [ ] `page.evaluate_handle()` — remote object references
- [ ] Structured trace/HAR capture
- [ ] CI matrix: Linux + macOS + Windows / stable + beta Chrome
- [ ] Cross-platform: replace `nix` for Windows support

---

## License

Dual licensed under [MIT](LICENSE-MIT) OR Apache-2.0 at your option.
