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

Apples-to-apples, **same Chrome binary** (Chrome for Testing 131.0.6778.204), same machine, same Linux host, headless, warm browser unless noted, 20 iterations per metric, 3 runs, median of medians. Bench harnesses for every library live under [`bench/`](bench/); feel free to reproduce.

| Operation | ferrous-browser | Puppeteer | Playwright | chromiumoxide |
|-----------|----------------:|----------:|-----------:|--------------:|
| `launch_chrome` (cold) | 268 ms | 269 ms | **176 ms** | 217 ms |
| `new_page` (warm browser) | **52 ms** | 81 ms | 87 ms | 79 ms |
| `goto` (`about:blank`, warm) | 15 ms | 14 ms | 14 ms | **11 ms** |
| `screenshot` (PNG) | 50 ms | 50 ms | 50 ms | 50 ms |
| `evaluate` (`document.title`) | **0.13 ms** | 0.63 ms | 0.97 ms | 0.15 ms |
| `wait_for_selector` reaction gap¹ | **1.1 ms** | 4.9 ms | 26.3 ms | 16.6 ms² |

¹ *Reaction gap* is the time between an element being inserted into the DOM and `wait_for_selector` returning. This is the cost of polling vs. observing, and the difference users actually feel in real tests. See [Selector waits, in detail](#selector-waits-in-detail) below.

² chromiumoxide has no built-in `wait_for_selector`; the canonical user pattern is a manual retry loop. The number above uses `sleep(50 ms)` between checks, which is what its examples suggest.

### What this actually tells you

- **`launch_chrome`** is roughly even with Puppeteer and slower than Playwright (176 ms). Playwright's edge here is worth investigating; for now we read Chrome's `DevTools listening on ws://...` line off stderr instead of polling the `/json/version` HTTP endpoint, which removes a 200 ms backoff loop and the HTTP retry budget.
- **`new_page`** is where library design starts to show. ferrous-browser uses `Target.setAutoAttach` so a new tab's session is bound without a second roundtrip, and lazy-enables the `Page` domain exactly once per session rather than on every `goto` (saves one CDP round-trip per navigation; the win scales with RTT).
- **`goto`** to `about:blank` is dominated by Chrome (11–15 ms across the board). Real navigation is dominated by the network, not the library.
- **`screenshot`** is Chrome's own work; every library lands at exactly 50 ms. All four default to viewport-only PNG capture, which is why they cluster. Library overhead here is rounding error.
- **`evaluate`** is roughly 5x faster than the Node-based libraries because we don't pay the Node-to-Chrome IPC hop on top of Chrome's own latency; we're a single process talking straight to Chrome.
- **`wait_for_selector` reaction gap** is the biggest gap, and it's the one users notice on every test. ferrous-browser pushes the wait into the page itself via a MutationObserver-backed Promise that Chrome holds open until the selector matches, so reaction latency is bounded by one CDP round-trip rather than by anyone's poll interval.

### Selector waits, in detail

In real test suites and scrapers, `wait_for_selector` is called dozens to hundreds of times. Every extra millisecond of reaction latency stacks up, and most libraries lose tens of milliseconds per call to polling.

Here's how each library reacts to an element that gets inserted at a known instant in the page:

```
ferrous-browser   median 1.1 ms   max 1.3 ms     ← in-page MutationObserver, awaited via CDP
Puppeteer         median 4.9 ms   max ~10  ms    ← polls on requestAnimationFrame
chromiumoxide     median 16.6 ms  max ~30  ms    ← no built-in; user-written 50 ms poll loop
Playwright        median 26.3 ms  max ~35  ms    ← internal polling, ~25 ms cadence
```

So on a test that does 100 `waitFor`s, ferrous-browser saves roughly **2.5 seconds vs Playwright** and **1.6 seconds vs chromiumoxide** purely from lower reaction latency, with no change in your code.

### Earlier benchmarks (macOS, Chrome 147)

Kept for continuity; these numbers used a different rig (macOS Apple Silicon, system-installed Chrome 147) and a smaller library set, so they are **not directly comparable** to the Linux/CfT table above.

| Operation | ferrous-browser | Puppeteer | chromiumoxide |
|-----------|-----------------|-----------|---------------|
| **New Page** (`about:blank`) | ~466 ms | ~75 ms | ~100 ms |
| **Navigate + Content** (`example.com`, load event) | ~735 ms | ~314 ms | ~277 ms |
| **Screenshot** (Full page PNG) | ~646 ms | ~138 ms | ~180 ms |

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

- [x] `page.set_cookies()` / `page.cookies()` — session persistence
- [x] `page.pdf()` — PDF export
- [x] `page.evaluate_handle()` — remote object references
- [ ] Structured trace/HAR capture
- [ ] CI matrix: Linux + macOS + Windows / stable + beta Chrome
- [ ] Cross-platform: replace `nix` for Windows support

---

## License

Dual licensed under [MIT](LICENSE-MIT) OR Apache-2.0 at your option.
