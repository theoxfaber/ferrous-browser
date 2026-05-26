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

| Operation | ferrous-browser | Puppeteer | Playwright | chromiumoxide | headless_chrome |
|-----------|----------------:|----------:|-----------:|--------------:|----------------:|
| `launch_chrome` (cold) | 357 ms | 162 ms | **93 ms** | 134 ms | 239 ms |
| `new_page` (warm browser) | **14 ms** | 23 ms | 28 ms | 24 ms | 517 ms³ |
| `goto` (`about:blank`, warm) | 6.2 ms | 5.1 ms | 4.7 ms | **4.3 ms** | 2137 ms³ |
| `screenshot` (PNG) | **37 ms** | 41 ms | 50 ms | 38 ms | 120 ms |
| `evaluate` (`document.title`) | 0.22 ms | 0.45 ms | 0.79 ms | **0.18 ms** | 104 ms³ |
| `wait_for_selector` reaction gap¹ | **1.1 ms** | 3.4 ms | 102 ms | 17.5 ms² | 2404 ms³ |

¹ *Reaction gap* is the time between an element being inserted into the DOM and `wait_for_selector` returning. This is the cost of polling vs. observing, and the difference users actually feel in real tests. See [Selector waits, in detail](#selector-waits-in-detail) below.

² chromiumoxide has no built-in `wait_for_selector`; the canonical user pattern is a manual retry loop. The number above uses `sleep(50 ms)` between checks, which is what its examples suggest.

³ `headless_chrome` ships a synchronous API whose internal transport polls the websocket response channel every 5 ms and whose `Wait` primitives default to a 100 ms sleep. `wait_until_navigated` waits for `networkAlmostIdle` (no public option for `load`-only), so its `goto` measurement isn't directly comparable to the `waitUntil: 'load'` semantics used by the other rows. The floor on `evaluate` (~104 ms) is one poll cycle of that internal `Wait`.

### What this actually tells you

- **`launch_chrome`** is slower than Playwright and Puppeteer on this run. The Node libraries skip a chunk of in-process setup that the Rust crates pay synchronously. ferrous reads Chrome's `DevTools listening on ws://...` line off stderr instead of polling the `/json/version` HTTP endpoint, which removes a 200 ms backoff loop, but there is still room to close the gap vs Playwright.
- **`new_page`** is where library design starts to show. ferrous-browser uses `Target.setAutoAttach` so a new tab's session is bound without a second roundtrip, and lazy-enables the `Page` domain exactly once per session rather than on every `goto` (saves one CDP round-trip per navigation; the win scales with RTT). `headless_chrome`'s 517 ms here is its sync transport waiting on the new-target attachment via its 100 ms `Wait` primitive.
- **`goto`** to `about:blank` is dominated by Chrome (4–6 ms across the modern async libraries). Real navigation is dominated by the network, not the library. `headless_chrome`'s 2.1 s is not slow Chrome; it's its sync `wait_until_navigated` waiting for `networkAlmostIdle` through a 100 ms-resolution polling loop.
- **`screenshot`** is mostly Chrome's own work; the four modern libraries land between 37 and 50 ms. Library overhead here is small. `headless_chrome` is ~3x slower because each CDP method call goes through its polling transport.
- **`evaluate`** in ferrous, Puppeteer, Playwright, and chromiumoxide is sub-millisecond — they all do a single CDP round-trip and pick up the response off an event loop or channel. `headless_chrome`'s 104 ms is *exactly* one cycle of its internal 100 ms `Wait` sleep.
- **`wait_for_selector` reaction gap** is the biggest gap among the async libraries, and it's the one users notice on every test. ferrous-browser pushes the wait into the page itself via a MutationObserver-backed Promise that Chrome holds open until the selector matches, so reaction latency is bounded by one CDP round-trip rather than by anyone's poll interval.

### Selector waits, in detail

In real test suites and scrapers, `wait_for_selector` is called dozens to hundreds of times. Every extra millisecond of reaction latency stacks up, and most libraries lose tens of milliseconds per call to polling.

Here's how each library reacts to an element that gets inserted at a known instant in the page:

```
ferrous-browser   median 1.1 ms     max ~1.3 ms     ← in-page MutationObserver, awaited via CDP
Puppeteer         median 3.4 ms     max ~5   ms     ← polls on requestAnimationFrame
chromiumoxide     median 17.5 ms    max ~30  ms     ← no built-in; user-written 50 ms poll loop
Playwright        median 102 ms     max ~105 ms     ← internal polling, sits on a 100 ms cadence
headless_chrome   median 2,404 ms   max ~2.4 s      ← sync transport, 100 ms Wait cycles compound
```

So on a test that does 100 `waitFor`s, ferrous-browser saves roughly **10 seconds vs Playwright**, **1.6 seconds vs chromiumoxide**, and minutes vs `headless_chrome` purely from lower reaction latency, with no change in your code.

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
| Async runtime | tokio | tokio | none (sync) |
| Node.js required | ❌ | ❌ | ❌ |
| Actively maintained | ✅ | ⚠️ stale | ✅ (community fork)¹ |
| Multi-page session isolation | ✅ | ✅ | ⚠️ |
| `page.evaluate::<T>()` | ✅ | ✅ | ⚠️ returns `RemoteObject` |
| Locator API | ✅ | ❌ | ❌ |
| `WaitUntil::NetworkIdle` | ✅ configurable | ❌ | ⚠️ hard-coded only |
| Structured errors | ✅ | ⚠️ | ⚠️ |

¹ The original `atroche/rust-headless-chrome` stopped seeing commits in Feb 2024; the crate is now maintained by the `rust-headless-chrome` GitHub org, latest release `1.0.21` on 2026-02-03. Note that its sync transport polls at 100 ms — see the benchmark footnote.

---

## Roadmap

- [x] `page.set_cookies()` / `page.cookies()` — session persistence
- [x] `page.pdf()` — PDF export
- [x] `page.evaluate_handle()` — remote object references
- [x] Structured trace/HAR capture
- [x] CI matrix: Linux + macOS + Windows / stable + beta Chrome
- [x] Cross-platform: replace `nix` for Windows support

---

## License

Dual licensed under [MIT](LICENSE-MIT) OR Apache-2.0 at your option.
