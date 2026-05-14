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
    chrome_path: Some("/usr/bin/chromium".into()), // optional explicit binary
    args: vec!["--disable-extensions".to_string()],
};

let browser = Browser::launch_chrome(Some(config)).await?;
```

| Field | Default | Description |
|-------|---------|-------------|
| `headless` | `true` | Headless mode |
| `timeout` | 30 s | Chrome startup deadline |
| `viewport` | 1280×720 | Window size in logical pixels |
| `chrome_path` | auto-discover | Explicit Chrome/Chromium executable path |
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

Apples-to-apples, **same Chrome binary** (Chrome for Testing 131.0.6778.204), same machine, same Linux host, headless, warm browser unless noted. The public numbers below are median-of-medians across **3 independent runs**. Bench harnesses for every library live under [`bench/`](bench/); feel free to reproduce.

### Hot-path matrix

| Operation | ferrous-browser | Puppeteer | Playwright | chromiumoxide | headless_chrome |
|-----------|----------------:|----------:|-----------:|--------------:|----------------:|
| `launch_chrome` (cold) | 137.3 ms | 161.0 ms | **90.4 ms** | 125.5 ms | 237.7 ms |
| `new_page` (warm browser) | **13.6 ms** | 24.7 ms | 25.6 ms | 22.1 ms | 517.5 ms³ |
| `goto` (`about:blank`, warm) | 4.2 ms | 4.8 ms | 4.3 ms | **4.1 ms** | 2137.1 ms³ |
| `screenshot` (PNG) | 49.8 ms | 38.9 ms | 49.4 ms | **38.0 ms** | 125.7 ms |
| `evaluate` (`document.title`) | **0.1 ms** | 0.3 ms | 0.6 ms | 0.2 ms | 104.0 ms³ |
| `wait_for_selector` reaction gap¹ | **1.0 ms** | 3.5 ms | 113.0 ms | 16.8 ms² | 2402.0 ms³ |
| `networkidle_static` | **19.1 ms** | 2016.8 ms | 503.0 ms | n/a² | 2215.9 ms |
| `networkidle_deferred_250` | **316.6 ms** | 2016.7 ms | 502.4 ms | n/a² | 2199.7 ms |
| `wait_for_function` gap | **9.9 ms** | 10.2 ms | 11.1 ms | 27.7 ms² | 500.6 ms³ |
| `click_when_enabled` gap | **0.4 ms** | 28.1 ms | 38.0 ms | 58.9 ms² | 1331.9 ms³ |

### Realistic-flow matrix

Deterministic local fixtures, same median-of-medians method:

| Scenario metric | ferrous-browser | Puppeteer | Playwright | chromiumoxide | headless_chrome |
|----------------|----------------:|----------:|-----------:|--------------:|----------------:|
| `todomvc_boot_ready` | 288.0 ms | **283.4 ms** | 296.4 ms | 327.2 ms² | 1558.9 ms³ |
| `todomvc_full_flow` | **695.1 ms** | 766.6 ms | 1310.2 ms | 902.8 ms² | 17768.0 ms³ |
| `todomvc_settled_screenshot` | 245.7 ms | **199.8 ms** | 342.7 ms | 201.9 ms² | 1403.1 ms³ |
| `conduit_login_ready` | 337.0 ms | 332.7 ms | 346.1 ms | **332.5 ms²** | 952.0 ms³ |
| `conduit_auth_article_flow` | **886.4 ms** | 964.4 ms | 1078.1 ms | 1076.5 ms² | 11521.2 ms³ |
| `conduit_article_settled_screenshot` | 418.1 ms | **337.2 ms** | 436.6 ms | 357.2 ms² | 1629.3 ms³ |

¹ *Reaction gap* is the time between an element being inserted into the DOM and `wait_for_selector` returning. This is the cost of polling vs. observing, and the difference users actually feel in real tests. See [Selector waits, in detail](#selector-waits-in-detail) below.

² chromiumoxide has no first-class `NetworkIdle` wait, no built-in `wait_for_selector`, and its realistic-flow waits in this matrix use a manual `sleep(50 ms)` retry cadence. Those rows are representative of the canonical user pattern, not hidden library magic.

³ `headless_chrome` ships a synchronous API whose internal transport polls the websocket response channel every 5 ms and whose `Wait` primitives default to a 100 ms sleep. `wait_until_navigated` waits for `networkAlmostIdle` (no public option for `load`-only), `wait_for_element` uses built-in polling, and the realistic-flow waits here use manual `50 ms` polling. Its rows are useful as a practical baseline, but several are not directly comparable to the async libraries' semantics.

### What this actually tells you

- **`launch_chrome`** is no longer the glaring problem it was earlier in this project. After pinning every harness to the same Chrome-for-Testing binary and tightening default launch flags, ferrous-browser now beats Puppeteer on this Linux rig and trails Playwright by ~47 ms.
- **`new_page`** is one of ferrous-browser's clearest wins. `Target.setAutoAttach` plus one-time lazy `Page.enable` keeps new-tab setup to ~14 ms, materially faster than the other modern libraries.
- **`goto`** to `about:blank` is dominated by Chrome (4–5 ms across the modern async libraries). Real navigation is network- and page-work-bound, not library-bound. `headless_chrome`'s ~2.1 s is its sync `wait_until_navigated` semantics, not Chrome itself.
- **`networkidle_*`** is where the composite wait design matters. ferrous-browser returns in ~19 ms on a truly idle page and ~317 ms on a page with a deferred `setTimeout(..., 250)`. Playwright sits around ~503 ms and Puppeteer around ~2.0 s because their semantics are much coarser.
- **`wait_for_selector` reaction gap** is the most repeated tax in real test suites. ferrous-browser uses a MutationObserver-backed in-page Promise, so reaction latency is effectively one CDP round-trip instead of a polling cadence.
- **`wait_for_function`** is now competitive with the Node leaders, and **`click_when_enabled`** is where ferrous-browser's in-page actionability path is most obvious: ~0.4 ms versus tens of milliseconds for the other serious libraries.
- **The realistic rows are the important sanity check.** On interaction-heavy flows (`todomvc_full_flow`, `conduit_auth_article_flow`), ferrous-browser is the fastest library in the modern set. On screenshot-only rows, Puppeteer still has an edge, which is a useful reminder that screenshot timing is often more Chrome-bound than wait-primitive-bound.

### Selector waits, in detail

In real test suites and scrapers, `wait_for_selector` is called dozens to hundreds of times. Every extra millisecond of reaction latency stacks up, and most libraries lose tens of milliseconds per call to polling.

Here's how each library reacts to an element that gets inserted at a known instant in the page:

```
ferrous-browser   median 1.0 ms      ← in-page MutationObserver, awaited via CDP
Puppeteer         median 3.5 ms      ← polls on requestAnimationFrame
chromiumoxide     median 16.8 ms     ← no built-in; user-written 50 ms poll loop
Playwright        median 113.0 ms    ← internal polling, sits on a 100 ms cadence
headless_chrome   median 2,402.0 ms  ← sync transport and polling waits compound
```

So on a test that does 100 `waitFor`s, ferrous-browser saves roughly **11.2 seconds vs Playwright**, **1.6 seconds vs chromiumoxide**, and minutes vs `headless_chrome` purely from lower reaction latency, with no change in your code.

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
- [ ] Structured trace/HAR capture
- [ ] CI matrix: Linux + macOS + Windows / stable + beta Chrome
- [ ] Cross-platform: replace `nix` for Windows support

---

## License

Dual licensed under [MIT](LICENSE-MIT) OR Apache-2.0 at your option.
