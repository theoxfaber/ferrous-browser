use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::time::{timeout, Duration};

use crate::cdp::CDPClient;
use crate::error::{BrowserError, Result};

// ─── P2: WaitUntil enum ──────────────────────────────────────────────────────

/// Controls when [`Page::goto`] considers navigation complete.
#[derive(Debug, Clone, Copy, Default)]
pub enum WaitUntil {
    /// Wait for `Page.domContentEventFired` — the DOM is parsed but
    /// sub-resources (images, stylesheets) may still be loading.
    DomContentLoaded,
    /// Wait for `Page.loadEventFired` — all resources have loaded.
    /// This is the default.
    #[default]
    Load,
    /// Wait until there are no in-flight network requests for 500 ms.
    /// Useful for SPAs that fetch data after the load event.
    NetworkIdle,
}

// ─── P3: Locator ─────────────────────────────────────────────────────────────

/// A lazy handle to a DOM element identified by a CSS selector.
///
/// Locators are created with [`Page::locator`] and make the common
/// "find-then-act" pattern ergonomic and composable.
///
/// # Example
///
/// ```no_run
/// # use ferrous_browser::{Browser, WaitUntil};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let browser = Browser::launch().await?;
/// let page = browser.new_page().await?;
/// page.goto("https://example.com", WaitUntil::Load).await?;
///
/// // Locator API
/// page.locator("button#submit").click().await?;
/// page.locator("input[name=q]").type_text("hello").await?;
/// page.locator(".result").wait_for().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Locator {
    selector: String,
    page: Page,
}

impl Locator {
    fn new(selector: impl Into<String>, page: Page) -> Self {
        Self {
            selector: selector.into(),
            page,
        }
    }

    /// Click the element identified by this locator.
    pub async fn click(&self) -> Result<()> {
        self.page.click_selector(&self.selector).await
    }

    /// Type text into the element identified by this locator.
    pub async fn type_text(&self, text: &str) -> Result<()> {
        self.page.type_text_selector(&self.selector, text).await
    }

    /// Wait until the element is present in the DOM (30 s default timeout).
    pub async fn wait_for(&self) -> Result<()> {
        self.page.wait_for_selector(&self.selector).await
    }

    /// Wait until the element is present with a custom timeout.
    pub async fn wait_for_timeout(&self, dur: Duration) -> Result<()> {
        self.page.wait_for_selector_with_timeout(&self.selector, dur).await
    }

    /// Get the inner text of the element.
    pub async fn inner_text(&self) -> Result<String> {
        let expr = format!("document.querySelector('{}')?.innerText ?? ''", escape_selector(&self.selector));
        let result = self.page.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": expr, "returnByValue": true })),
        ).await?;
        result
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| BrowserError::invalid_response(
                format!("inner_text('{}')", self.selector),
                "unexpected result shape",
            ))
    }

    /// Get an attribute value of the element.
    pub async fn get_attribute(&self, name: &str) -> Result<Option<String>> {
        let expr = format!(
            "document.querySelector('{}')?.getAttribute('{}') ?? null",
            escape_selector(&self.selector),
            name,
        );
        let result = self.page.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": expr, "returnByValue": true })),
        ).await?;
        let val = result
            .get("result")
            .and_then(|r| r.get("value"));
        match val {
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(Value::Null) | None => Ok(None),
            _ => Ok(val.map(|v| v.to_string())),
        }
    }
}

// ─── Page ────────────────────────────────────────────────────────────────────

/// A handle to a single page/tab in the browser.
///
/// Page provides methods for interacting with a specific page or tab,
/// including navigation, content retrieval, screenshot capture, and
/// element interaction.
///
/// # Example
///
/// ```no_run
/// use ferrous_browser::{Browser, WaitUntil};
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let browser = Browser::launch().await?;
/// let page = browser.new_page().await?;
///
/// page.goto("https://example.com", WaitUntil::Load).await?;
/// let html = page.content().await?;
/// let screenshot = page.screenshot().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Page {
    /// Target/page ID
    pub target_id: String,
    /// Session ID for routing CDP commands
    pub session_id: String,
    /// Reference to CDP client
    cdp: Arc<CDPClient>,
}

impl Page {
    /// Create a new page handle
    #[doc(hidden)]
    pub fn new(target_id: String, session_id: String, cdp: Arc<CDPClient>) -> Self {
        Page {
            target_id,
            session_id,
            cdp,
        }
    }

    // ─── P3: Locator entry point ──────────────────────────────────────────

    /// Create a [`Locator`] for the given CSS selector.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    ///
    /// page.locator("button#submit").click().await?;
    /// page.locator("input[name=q]").type_text("rust").await?;
    /// page.locator(".result").wait_for().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn locator(&self, selector: &str) -> Locator {
        Locator::new(selector, self.clone())
    }

    // ─── P2: goto with WaitUntil ─────────────────────────────────────────

    /// Navigate to a URL and wait for the specified condition.
    ///
    /// # Arguments
    ///
    /// * `url`        — The URL to navigate to
    /// * `wait_until` — When to consider navigation complete
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// page.goto("https://example.com", WaitUntil::DomContentLoaded).await?;
    /// page.goto("https://example.com", WaitUntil::NetworkIdle).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn goto(&self, url: &str, wait_until: WaitUntil) -> Result<()> {
        const TIMEOUT_SECS: u64 = 30;
        let url_owned = url.to_string();
        // Capture session_id so the async block can own it
        let session_id = self.session_id.clone();

        let event_method = match wait_until {
            WaitUntil::DomContentLoaded => "Page.domContentEventFired",
            WaitUntil::Load | WaitUntil::NetworkIdle => "Page.loadEventFired",
        };

        // ── Subscribe BEFORE sending any command (race-condition fix) ─────────
        // Filter by BOTH method name AND session_id so concurrent pages never
        // receive each other's load events (multi-page isolation fix).
        let mut event_rx = self.cdp.subscribe_events();
        // ─────────────────────────────────────────────────────────────────────

        let _ = self.send_command("Page.enable".to_string(), None).await;

        let response = self.send_command(
            "Page.navigate".to_string(),
            Some(json!({ "url": url })),
        ).await?;

        if let Some(error_text) = response.get("errorText").and_then(|v| v.as_str()) {
            return Err(BrowserError::navigation_failed(&url_owned, error_text));
        }

        let wait_result = timeout(Duration::from_secs(TIMEOUT_SECS), async {
            match wait_until {
                WaitUntil::NetworkIdle => {
                    let mut last_activity = tokio::time::Instant::now();
                    loop {
                        tokio::select! {
                            recv = event_rx.recv() => {
                                match recv {
                                    Ok(msg)
                                        if msg.session_id.as_deref() == Some(&session_id) =>
                                    {
                                        last_activity = tokio::time::Instant::now();
                                    }
                                    Ok(_) => {} // different session
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                        last_activity = tokio::time::Instant::now();
                                    }
                                    Err(_) => {}
                                }
                            }
                            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                                if last_activity.elapsed() >= Duration::from_millis(500) {
                                    return Ok::<(), BrowserError>(());
                                }
                            }
                        }
                    }
                }
                _ => loop {
                    match event_rx.recv().await {
                        Ok(msg)
                            if msg.method.as_deref() == Some(event_method)
                                && msg.session_id.as_deref() == Some(&session_id) =>
                        {
                            return Ok(());
                        }
                        Ok(_) => {} // wrong session or wrong event
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            return Ok(()); // assume fired
                        }
                        Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                    }
                },
            }
        })
        .await;

        wait_result.map_err(|_| BrowserError::timeout(
            format!("navigating to '{}'", url_owned),
            TIMEOUT_SECS,
        ))?
    }

    // ─── evaluate ─────────────────────────────────────────────────────────

    /// Evaluate a JavaScript expression in the page context and deserialize the
    /// result as `T`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch_chrome(None).await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let title: String = page.evaluate("document.title").await?;
    /// let count: u64 = page.evaluate("document.querySelectorAll('a').length").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn evaluate<T: DeserializeOwned>(&self, expression: &str) -> Result<T> {
        let result = self.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
            })),
        ).await?;

        if let Some(exc) = result.get("exceptionDetails") {
            let msg = exc
                .get("exception")
                .and_then(|e| e.get("description"))
                .and_then(|d| d.as_str())
                .unwrap_or("unknown JS exception");
            return Err(BrowserError::command_failed("Runtime.evaluate", msg));
        }

        let value = result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null);

        serde_json::from_value(value)
            .map_err(|e| BrowserError::invalid_response("evaluate()", e.to_string()))
    }

    // ─── Wait helpers ─────────────────────────────────────────────────────

    /// Wait for an element matching `selector` to appear in the DOM.
    ///
    /// Uses a 30-second timeout.
    pub async fn wait_for_selector(&self, selector: &str) -> Result<()> {
        self.wait_for_selector_with_timeout(selector, Duration::from_secs(30)).await
    }

    /// Wait for an element matching `selector` with a custom timeout.
    pub async fn wait_for_selector_with_timeout(&self, selector: &str, dur: Duration) -> Result<()> {
        let selector = selector.to_string();
        let timeout_secs = dur.as_secs();

        let fut = async {
            loop {
                let expr = format!(
                    "!!document.querySelector('{}')",
                    escape_selector(&selector),
                );
                let result = self.send_command(
                    "Runtime.evaluate".to_string(),
                    Some(json!({ "expression": expr, "returnByValue": true })),
                ).await?;

                if let Some(true) = result
                    .get("result")
                    .and_then(|r| r.get("value"))
                    .and_then(|v| v.as_bool())
                {
                    return Ok::<(), BrowserError>(());
                }

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };

        timeout(dur, fut).await.map_err(|_| BrowserError::timeout(
            format!("waiting for selector '{}'", selector),
            timeout_secs,
        ))?
    }

    // ─── Interaction helpers (internal, also used by Locator) ─────────────

    /// Click an element matching the selector (internal implementation).
    pub(crate) async fn click_selector(&self, selector: &str) -> Result<()> {
        let expr = format!(
            "document.querySelector('{}').click()",
            escape_selector(selector),
        );
        self.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": expr })),
        ).await?;
        Ok(())
    }

    /// Type text into an element (internal implementation).
    pub(crate) async fn type_text_selector(&self, selector: &str, text: &str) -> Result<()> {
        let focus_expr = format!("document.querySelector('{}').focus()", escape_selector(selector));
        self.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": focus_expr })),
        ).await?;

        for ch in text.chars() {
            self.send_command(
                "Input.dispatchKeyEvent".to_string(),
                Some(json!({
                    "type": "char",
                    "text": ch.to_string(),
                })),
            ).await?;
        }
        Ok(())
    }

    // ─── Public raw-selector methods (legacy / power-user API) ────────────

    /// Click an element matching the CSS selector.
    ///
    /// Prefer [`Page::locator`] for new code.
    pub async fn click(&self, selector: &str) -> Result<()> {
        self.click_selector(selector).await
    }

    /// Type text into an input element matching the CSS selector.
    ///
    /// Prefer [`Page::locator`] for new code.
    pub async fn type_text(&self, selector: &str, text: &str) -> Result<()> {
        self.type_text_selector(selector, text).await
    }

    // ─── Content / screenshot ────────────────────────────────────────────

    /// Get the full HTML content of the page.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let html = page.content().await?;
    /// println!("HTML: {}", html);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn content(&self) -> Result<String> {
        let result = self.send_command(
            "Runtime.evaluate".to_string(),
            Some(json!({ "expression": "document.documentElement.outerHTML" })),
        ).await?;

        result
            .get("result")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| BrowserError::invalid_response("content()", "missing result.value string"))
    }

    /// Take a screenshot of the page and return PNG bytes.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use ferrous_browser::{Browser, WaitUntil};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// let page = browser.new_page().await?;
    /// page.goto("https://example.com", WaitUntil::Load).await?;
    /// let png = page.screenshot().await?;
    /// std::fs::write("screenshot.png", png)?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn screenshot(&self) -> Result<Vec<u8>> {
        let result = self.send_command(
            "Page.captureScreenshot".to_string(),
            None,
        ).await?;

        let base64_data = result
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| BrowserError::invalid_response("screenshot()", "missing data field"))?;

        base64_decode(base64_data)
    }

    // ─── Network interception ────────────────────────────────────────────

    /// Intercept network requests matching a pattern.
    ///
    /// Enables request interception and calls the callback for matching
    /// requests. The callback receives `(url, resource_type)` and returns
    /// `true` to abort the request.
    pub async fn intercept_requests<F>(&self, callback: F) -> Result<()>
    where
        F: Fn(&str, &str) -> bool + Send + 'static,
    {
        let _ = self.send_command("Network.enable".to_string(), None).await;
        let _ = self.send_command(
            "Network.setRequestInterception".to_string(),
            Some(json!({ "patterns": [{ "urlPattern": "*" }] })),
        ).await;

        // ── P1: Subscribe BEFORE the enable command fires events ─────────────
        let mut event_rx = self.cdp.subscribe_events();
        // ────────────────────────────────────────────────────────────────────

        let cdp = self.cdp.clone();
        let session_id = self.session_id.clone();
        tokio::spawn(async move {
            while let Ok(msg) = event_rx.recv().await {
                // Only handle Network.requestIntercepted for this page's session
                if msg.method.as_deref() != Some("Network.requestIntercepted") {
                    continue;
                }
                if msg.session_id.as_deref() != Some(&session_id) {
                    continue;
                }
                if let Some(params) = msg.params {
                    let url = params
                        .get("request")
                        .and_then(|r| r.get("url"))
                        .and_then(|u| u.as_str())
                        .unwrap_or("");
                    let resource_type = params
                        .get("request")
                        .and_then(|r| r.get("resourceType"))
                        .and_then(|r| r.as_str())
                        .unwrap_or("");
                    let request_id = params
                        .get("requestId")
                        .and_then(|r| r.as_str())
                        .unwrap_or("");

                    let should_abort = callback(url, resource_type);

                    let cdp_method = if should_abort {
                        "Network.abortRequest"
                    } else {
                        "Network.continueInterceptedRequest"
                    };

                    let _ = cdp
                        .send_command_with_session(
                            &session_id,
                            cdp_method.to_string(),
                            Some(json!({ "requestId": request_id })),
                        )
                        .await;
                }
            }
        });

        Ok(())
    }

    // ─── Internal ─────────────────────────────────────────────────────────

    /// Send a command to this page's session
    pub(crate) async fn send_command(&self, method: String, params: Option<Value>) -> Result<Value> {
        self.cdp.send_command_with_session(&self.session_id, method, params).await
    }
}

// ─── Utilities ────────────────────────────────────────────────────────────────

/// Escape single-quotes in a CSS selector used inside JS string literals.
fn escape_selector(s: &str) -> String {
    s.replace('\'', "\\'")
}

/// Decode base64 string to bytes
fn base64_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    engine
        .decode(s)
        .map_err(|e| BrowserError::invalid_response("screenshot()", format!("base64 decode failed: {e}")))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_until_default() {
        let w: WaitUntil = Default::default();
        assert!(matches!(w, WaitUntil::Load));
    }

    #[test]
    fn test_escape_selector_plain() {
        assert_eq!(escape_selector("button#id"), "button#id");
    }

    #[test]
    fn test_escape_selector_quotes() {
        assert_eq!(escape_selector("input[name='q']"), "input[name=\\'q\\']");
    }
}