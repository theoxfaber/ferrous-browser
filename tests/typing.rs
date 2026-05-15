use std::ops::Deref;
use std::sync::OnceLock;
use std::time::Duration;

use ferrous_browser::{Browser, BrowserError, WaitUntil};
use serde::Deserialize;

struct LaunchedBrowser {
    browser: Browser,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl Deref for LaunchedBrowser {
    type Target = Browser;

    fn deref(&self) -> &Self::Target {
        &self.browser
    }
}

fn chrome_test_gate() -> std::sync::Arc<tokio::sync::Semaphore> {
    static GATE: OnceLock<std::sync::Arc<tokio::sync::Semaphore>> = OnceLock::new();
    GATE.get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)))
        .clone()
}

async fn launch_or_skip() -> Option<LaunchedBrowser> {
    let permit = chrome_test_gate()
        .acquire_owned()
        .await
        .expect("chrome test semaphore closed");

    match Browser::launch_chrome(None).await {
        Ok(browser) => Some(LaunchedBrowser {
            browser,
            _permit: permit,
        }),
        Err(BrowserError::BrowserNotLaunched { message, .. }) => {
            eprintln!("⊘ skipping: chrome not launchable: {message}");
            drop(permit);
            None
        }
        Err(e) => panic!("unexpected chrome launch failure: {e}"),
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn data_url(html: &str) -> String {
    format!("data:text/html,{}", urlencode(html))
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PayloadStatus {
    Ready,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct WaitValuePayload {
    status: PayloadStatus,
    count: u64,
}

#[tokio::test]
async fn type_text_sets_value_and_fires_input() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body>
<input class="field" />
<script>
window.__events = [];
document.querySelector('.field').addEventListener('input', (event) => {
  window.__events.push(event.target.value);
});
</script>
</body></html>"#;

    page.goto(&data_url(html), WaitUntil::Load).await.unwrap();
    page.locator(".field")
        .type_text("ferrous typed text")
        .await
        .unwrap();

    let value: String = page
        .evaluate("document.querySelector('.field').value")
        .await
        .unwrap();
    let events: Vec<String> = page.evaluate("window.__events").await.unwrap();

    assert_eq!(value, "ferrous typed text");
    assert!(
        !events.is_empty(),
        "typing should dispatch at least one input event"
    );
    assert_eq!(
        events.last().map(String::as_str),
        Some("ferrous typed text")
    );
}

#[tokio::test]
async fn wait_for_function_value_returns_payload_after_settle() {
    let Some(browser) = launch_or_skip().await else {
        return;
    };
    let page = browser.new_page().await.unwrap();

    let html = r#"<!doctype html><html><body data-ready="false">
<script>
window.__payload = { status: 'warming', count: 0 };
setTimeout(() => {
  document.body.dataset.ready = 'true';
  window.__payload = { status: 'ready', count: 3 };
}, 60);
</script>
</body></html>"#;

    page.goto(&data_url(html), WaitUntil::Load).await.unwrap();
    let payload: WaitValuePayload = page
        .wait_for_function_value(
            "document.body.dataset.ready === 'true'",
            "window.__payload",
            Duration::from_secs(2),
        )
        .await
        .unwrap();

    assert_eq!(
        payload,
        WaitValuePayload {
            status: PayloadStatus::Ready,
            count: 3,
        }
    );
}
