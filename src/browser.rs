use crate::cdp::CDPClient;
use crate::connection::Connection;
use crate::error::{BrowserError, Result};
use crate::page::Page;
use nix::unistd::Pid;
use serde_json::json;
use std::net::TcpListener;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use tracing::Instrument;

// ── P4: BrowserConfig ────────────────────────────────────────────────────────

/// Configuration options for launching a Chrome/Chromium instance.
///
/// Use [`BrowserConfig::default()`] to get sensible defaults, then
/// customise the fields you need.
///
/// # Example
///
/// ```no_run
/// use ferrous_browser::{Browser, BrowserConfig};
/// use std::time::Duration;
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = BrowserConfig {
///     headless: true,
///     timeout: Duration::from_secs(60),
///     viewport: (1920, 1080),
///     args: vec!["--disable-extensions".to_string()],
/// };
/// let browser = Browser::launch_chrome(Some(config)).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Run Chrome in headless mode (default: `true`).
    pub headless: bool,
    /// Maximum time to wait for Chrome to start (default: 30 s).
    pub timeout: Duration,
    /// Viewport size as `(width, height)` in logical pixels (default: `1280 x 720`).
    pub viewport: (u32, u32),
    /// Additional Chrome command-line arguments appended after the built-in flags.
    pub args: Vec<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            timeout: Duration::from_secs(30),
            viewport: (1280, 720),
            args: Vec::new(),
        }
    }
}

// ── Browser ──────────────────────────────────────────────────────────────────

/// A handle to a Chrome/Chromium browser instance.
///
/// # Example
///
/// ```no_run
/// use ferrous_browser::{Browser, WaitUntil};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let browser = Browser::launch_chrome(None).await?;
///     let page = browser.new_page().await?;
///     page.goto("https://example.com", WaitUntil::Load).await?;
///     Ok(())
/// }
/// ```
pub struct Browser {
    cdp: Arc<CDPClient>,
    pages: Arc<RwLock<Vec<Page>>>,
    _child_pid: Option<Pid>,
}

impl Browser {
    fn find_chrome() -> Option<String> {
        let candidates = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "google-chrome",
            "chromium-browser",
            "chromium",
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        ];
        for candidate in candidates {
            if std::path::Path::new(candidate).exists() || which::which(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
        None
    }

    /// Pick a free TCP port on localhost.
    fn free_port() -> Result<u16> {
        TcpListener::bind("127.0.0.1:0")
            .map(|l| l.local_addr().unwrap().port())
            .map_err(|e| {
                BrowserError::BrowserNotLaunched(format!("Could not find a free port: {e}"))
            })
    }

    /// Launch Chrome/Chromium and connect to it automatically.
    ///
    /// Pass `None` to use [`BrowserConfig::default`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ferrous_browser::{Browser, BrowserConfig};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch_chrome(None).await?;
    ///
    /// let config = BrowserConfig { headless: false, ..Default::default() };
    /// let browser = Browser::launch_chrome(Some(config)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn launch_chrome(config: Option<BrowserConfig>) -> Result<Self> {
        let span = tracing::info_span!("Browser::launch_chrome");
        let _enter = span.enter();
        let config = config.unwrap_or_default();

        let chrome_path = Self::find_chrome().ok_or_else(|| {
            BrowserError::BrowserNotLaunched(
                "Chrome/Chromium not found. Install Google Chrome or set a custom path via BrowserConfig::args.".to_string(),
            )
        })?;

        // Use a dynamically-assigned free port so multiple instances never conflict
        let port = Self::free_port()?;

        let mut chrome_args: Vec<String> = vec![
            format!("--remote-debugging-port={port}"),
            "--no-sandbox".to_string(),
            "--disable-gpu".to_string(),
            "--disable-dev-shm-usage".to_string(),
            format!("--window-size={},{}", config.viewport.0, config.viewport.1),
        ];
        if config.headless {
            chrome_args.push("--headless=new".to_string());
        }
        chrome_args.extend(config.args.iter().cloned());

        let mut child = tracing::info_span!("spawn_chrome").in_scope(|| {
            Command::new(&chrome_path)
                .args(&chrome_args)
                .stderr(Stdio::piped())
                .stdout(Stdio::null())
                .stdin(Stdio::null())
                // We manage Chrome's lifetime via SIGTERM in Drop, matching
                // the prior behavior, so don't let tokio kill it on drop.
                .kill_on_drop(false)
                .spawn()
                .map_err(|e| {
                    BrowserError::BrowserNotLaunched(format!(
                        "Failed to spawn Chrome at '{chrome_path}': {e}"
                    ))
                })
        })?;

        let pid = child.id().ok_or_else(|| {
            BrowserError::BrowserNotLaunched("Chrome exited before reporting a pid".to_string())
        })?;
        let pid = Pid::from_raw(pid as i32);

        // Chrome announces readiness on stderr as soon as the devtools server
        // is listening:
        //     DevTools listening on ws://127.0.0.1:<port>/devtools/browser/<id>
        // Read that and skip the HTTP-poll-and-200ms-sleep dance entirely.
        let stderr = child
            .stderr
            .take()
            .expect("stderr is piped");

        let ws_url = tokio::time::timeout(config.timeout, async {
            let mut reader = BufReader::new(stderr).lines();
            while let Some(line) = reader
                .next_line()
                .await
                .map_err(|e| BrowserError::BrowserNotLaunched(format!("stderr read failed: {e}")))?
            {
                const PREFIX: &str = "DevTools listening on ";
                if let Some(idx) = line.find(PREFIX) {
                    let url = line[idx + PREFIX.len()..].trim().to_string();
                    // Keep draining stderr so the pipe never fills up and
                    // blocks Chrome on a future write.
                    tokio::spawn(async move {
                        let mut reader = reader;
                        while let Ok(Some(_)) = reader.next_line().await {}
                    });
                    return Ok::<String, BrowserError>(url);
                }
            }
            Err(BrowserError::BrowserNotLaunched(
                "Chrome exited before announcing its DevTools port".to_string(),
            ))
        })
        .instrument(tracing::info_span!("wait_for_chrome_ready"))
        .await
        .map_err(|_| {
            BrowserError::BrowserNotLaunched(format!(
                "Chrome did not start within {}s",
                config.timeout.as_secs()
            ))
        })??;

        // The tokio Child handle is no longer needed; lifetime is managed via
        // the stored Pid + SIGTERM in Drop, exactly as before.
        drop(child);

        Self::connect_internal(ws_url, Some(pid)).await
    }

    /// Connect to a CDP WebSocket URL directly.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ferrous_browser::Browser;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::connect("ws://localhost:9222".to_string()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(ws_url: String) -> Result<Self> {
        Self::connect_internal(ws_url, None).await
    }

    /// Connect to a Chrome instance already running on `localhost:9222`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use ferrous_browser::Browser;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let browser = Browser::launch().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn launch() -> Result<Self> {
        Self::connect("ws://localhost:9222".to_string()).await
    }

    async fn connect_internal(ws_url: String, pid: Option<Pid>) -> Result<Self> {
        use futures_util::StreamExt;
        let cdp = Arc::new(CDPClient::new(ws_url));
        let ws_stream = cdp.connect().await?;
        let (sink, stream) = ws_stream.split();
        cdp.set_sink(sink).await;
        let conn = Connection::new(cdp.clone(), stream);
        tokio::spawn(conn.run());

        // Enable auto-attach so new targets connect instantly without round-trip
        cdp.send_command(
            "Target.setAutoAttach".to_string(),
            Some(json!({
                "autoAttach": true,
                "waitForDebuggerOnStart": false,
                "flatten": true
            })),
        )
        .await?;

        Ok(Browser {
            cdp,
            pages: Arc::new(RwLock::new(Vec::new())),
            _child_pid: pid,
        })
    }

    /// Create a new page/tab in the browser.
    #[tracing::instrument(level = "info", skip(self))]
    pub async fn new_page(&self) -> Result<Page> {
        // Subscribe to events BEFORE creating target so we don't miss attachedToTarget
        let mut event_rx = self.cdp.subscribe_events();

        let target_response = self
            .cdp
            .send_command(
                "Target.createTarget".to_string(),
                Some(json!({ "url": "about:blank" })),
            )
            .await?;

        let target_id = target_response
            .get("targetId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                BrowserError::invalid_response(
                    "new_page()",
                    "missing targetId in Target.createTarget response",
                )
            })?
            .to_string();

        // Wait for the automatic Target.attachedToTarget event for this targetId
        let target_id_for_span = target_id.clone();
        let session_id = async {
            loop {
                match event_rx.recv().await {
                    Ok(msg) if msg.method.as_deref() == Some("Target.attachedToTarget") => {
                        if let Some(params) = msg.params {
                            let msg_target_id = params
                                .get("targetInfo")
                                .and_then(|t| t.get("targetId"))
                                .and_then(|t| t.as_str());
                            if msg_target_id == Some(&target_id) {
                                if let Some(sess_id) =
                                    params.get("sessionId").and_then(|s| s.as_str())
                                {
                                    return Ok::<String, BrowserError>(sess_id.to_string());
                                }
                            }
                        }
                    }
                    Ok(_) => {} // ignore other events
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => {
                        return Err(BrowserError::invalid_response(
                            "new_page()",
                            "event channel closed before Target.attachedToTarget",
                        ));
                    }
                }
            }
        }
        .instrument(tracing::info_span!(
            "await_attachedToTarget",
            target_id = %target_id_for_span
        ))
        .await?;

        let page = Page::new(target_id, session_id, self.cdp.clone());
        self.pages.write().await.push(page.clone());
        Ok(page)
    }

    /// Get the number of open pages/tabs.
    pub async fn page_count(&self) -> usize {
        self.pages.read().await.len()
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        if let Some(pid) = self._child_pid {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::SIGTERM);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_config_defaults() {
        let cfg = BrowserConfig::default();
        assert!(cfg.headless);
        assert_eq!(cfg.viewport, (1280, 720));
        assert_eq!(cfg.timeout, Duration::from_secs(30));
        assert!(cfg.args.is_empty());
    }

    #[test]
    fn test_browser_config_custom() {
        let cfg = BrowserConfig {
            headless: false,
            timeout: Duration::from_secs(60),
            viewport: (1920, 1080),
            args: vec!["--disable-extensions".to_string()],
        };
        assert!(!cfg.headless);
        assert_eq!(cfg.viewport, (1920, 1080));
        assert_eq!(cfg.timeout, Duration::from_secs(60));
        assert_eq!(cfg.args, vec!["--disable-extensions"]);
    }

    #[test]
    fn test_free_port() {
        let port = Browser::free_port().unwrap();
        assert!(port > 1024);
    }
}
