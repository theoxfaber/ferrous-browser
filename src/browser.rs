use crate::cdp::{spawn_writer_task, CDPClient};
use crate::connection::Connection;
use crate::error::{BrowserError, BrowserLaunchErrorKind, Result};
use crate::page::Page;
use backon::{ExponentialBuilder, Retryable};
use nix::libc::{signal, SIGPIPE, SIG_ERR, SIG_IGN};
use nix::unistd::Pid;
use serde_json::json;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
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
///     chrome_path: None,
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
    /// Explicit Chrome/Chromium executable path or command name.
    ///
    /// If unset, `Browser::launch_chrome()` first checks the `CHROME_PATH`
    /// environment variable and then falls back to common Chrome/Chromium
    /// installation locations.
    pub chrome_path: Option<String>,
    /// Additional Chrome command-line arguments appended after the built-in flags.
    pub args: Vec<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            timeout: Duration::from_secs(30),
            viewport: (1280, 720),
            chrome_path: None,
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
    chrome_user_data_dir: Option<PathBuf>,
}

struct SpawnedChrome {
    child: tokio::process::Child,
    pid: Pid,
    ws_url: String,
    user_data_dir: PathBuf,
}

impl Browser {
    const CHROME_LAUNCH_ATTEMPTS: usize = 3;
    const CHROME_LAUNCH_RETRY_MIN_DELAY: Duration = Duration::from_millis(150);
    const CHROME_LAUNCH_RETRY_MAX_DELAY: Duration = Duration::from_millis(300);
    const CHROME_STDERR_TAIL_LINES: usize = 8;
    const CHROME_READY_POLL_INTERVAL: Duration = Duration::from_millis(25);

    fn find_chrome() -> Option<String> {
        static CHROME_PATH_CACHE: OnceLock<Option<String>> = OnceLock::new();
        let candidates = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "google-chrome",
            "chromium-browser",
            "chromium",
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        ];
        CHROME_PATH_CACHE
            .get_or_init(|| {
                for candidate in candidates {
                    if Self::chrome_exists(candidate) {
                        return Some(candidate.to_string());
                    }
                }
                None
            })
            .clone()
    }

    fn chrome_exists(candidate: &str) -> bool {
        std::path::Path::new(candidate).exists() || which::which(candidate).is_ok()
    }

    fn resolve_chrome_path(config: &BrowserConfig) -> Result<String> {
        if let Some(path) = config.chrome_path.as_deref() {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                return Err(BrowserError::browser_not_launched(
                    BrowserLaunchErrorKind::ConfigInvalid,
                    "BrowserConfig.chrome_path was set but empty",
                ));
            }
            if Self::chrome_exists(trimmed) {
                return Ok(trimmed.to_string());
            }
            return Err(BrowserError::browser_not_launched(
                BrowserLaunchErrorKind::NotFound,
                format!("Chrome/Chromium not found at BrowserConfig.chrome_path='{trimmed}'"),
            ));
        }

        if let Ok(path) = std::env::var("CHROME_PATH") {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                if Self::chrome_exists(trimmed) {
                    return Ok(trimmed.to_string());
                }
                return Err(BrowserError::browser_not_launched(
                    BrowserLaunchErrorKind::NotFound,
                    format!("Chrome/Chromium not found at CHROME_PATH='{trimmed}'"),
                ));
            }
        }

        Self::find_chrome().ok_or_else(|| {
            BrowserError::browser_not_launched(
                BrowserLaunchErrorKind::NotFound,
                "Chrome/Chromium not found. Install Google Chrome, set CHROME_PATH, or set BrowserConfig.chrome_path.",
            )
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

        let chrome_path = Self::resolve_chrome_path(&config)?;
        let retry_policy = ExponentialBuilder::default()
            .with_factor(2.0)
            .with_min_delay(Self::CHROME_LAUNCH_RETRY_MIN_DELAY)
            .with_max_delay(Self::CHROME_LAUNCH_RETRY_MAX_DELAY)
            .with_max_times(Self::CHROME_LAUNCH_ATTEMPTS.saturating_sub(1));
        let mut next_attempt = 2usize;

        (|| async {
            let mut spawned = Self::launch_chrome_once(&chrome_path, &config).await?;
            match Self::connect_internal(
                spawned.ws_url.clone(),
                Some(spawned.pid),
                Some(spawned.user_data_dir.clone()),
            )
            .await
            {
                Ok(browser) => Ok(browser),
                Err(err) => {
                    Self::cleanup_failed_launch(&mut spawned.child, Some(&spawned.user_data_dir))
                        .await;
                    Err(BrowserError::browser_not_launched(
                        BrowserLaunchErrorKind::ConnectFailed,
                        err.to_string(),
                    ))
                }
            }
        })
        .retry(retry_policy)
        .sleep(tokio::time::sleep)
        .when(Self::should_retry_launch)
        .notify(|err: &BrowserError, delay| {
            tracing::warn!(
                attempt = next_attempt,
                max_attempts = Self::CHROME_LAUNCH_ATTEMPTS,
                retry_in_ms = delay.as_millis(),
                error = %err,
                "transient Chrome launch failure; retrying"
            );
            next_attempt += 1;
        })
        .await
    }

    async fn launch_chrome_once(
        chrome_path: &str,
        config: &BrowserConfig,
    ) -> Result<SpawnedChrome> {
        const BASE_ARGS: &[&str] = &[
            "--remote-debugging-port=0",
            "--no-sandbox",
            "--disable-gpu",
            "--disable-background-networking",
            "--disable-dev-shm-usage",
            "--disable-breakpad",
            "--disable-component-extensions-with-background-pages",
            "--disable-component-update",
            "--disable-default-apps",
            "--disable-extensions",
            // Without these, background tabs (anything not foreground) get
            // their timers and rAF callbacks throttled or paused. That breaks
            // composite NetworkIdle's rAF flush whenever multiple pages run
            // concurrently — the flush awaits a rAF tick that may never fire
            // on a backgrounded tab. Standard practice across Puppeteer /
            // Playwright; safe for automation use cases.
            "--disable-background-timer-throttling",
            "--disable-renderer-backgrounding",
            "--disable-backgrounding-occluded-windows",
            "--disable-search-engine-choice-screen",
            "--disable-sync",
            "--enable-automation",
            "--metrics-recording-only",
            "--no-default-browser-check",
            "--no-first-run",
            "--password-store=basic",
            "--use-mock-keychain",
        ];
        let mut chrome_args: Vec<Cow<'static, str>> =
            Vec::with_capacity(BASE_ARGS.len() + config.args.len() + 4);
        chrome_args.extend(BASE_ARGS.iter().copied().map(Cow::Borrowed));
        let user_data_dir = Self::create_chrome_user_data_dir()?;
        chrome_args.push(Cow::Owned(format!(
            "--user-data-dir={}",
            user_data_dir.display()
        )));
        chrome_args.push(Cow::Owned(format!(
            "--window-size={},{}",
            config.viewport.0, config.viewport.1
        )));
        if config.headless {
            chrome_args.extend(
                ["--headless=new", "--hide-scrollbars", "--mute-audio"]
                    .into_iter()
                    .map(Cow::Borrowed),
            );
        }
        chrome_args.extend(config.args.iter().cloned().map(Cow::Owned));

        let mut child = tracing::info_span!("spawn_chrome").in_scope(|| {
            let mut command = Command::new(chrome_path);
            command
                .args(chrome_args.iter().map(|arg| arg.as_ref()))
                .stderr(Stdio::piped())
                .stdout(Stdio::null())
                .stdin(Stdio::null())
                // We manage Chrome's lifetime via SIGTERM in Drop, matching
                // the prior behavior, so don't let tokio kill it on drop.
                .kill_on_drop(false);
            unsafe {
                command.pre_exec(|| {
                    if signal(SIGPIPE, SIG_IGN) == SIG_ERR {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            command.spawn().map_err(|e| {
                BrowserError::browser_not_launched(
                    BrowserLaunchErrorKind::SpawnFailed,
                    format!("Failed to spawn Chrome at '{chrome_path}': {e}"),
                )
            })
        })?;

        let pid = child.id().ok_or_else(|| {
            BrowserError::browser_not_launched(
                BrowserLaunchErrorKind::MissingPid,
                "Chrome exited before reporting a pid",
            )
        })?;
        let pid = Pid::from_raw(pid as i32);

        // Chrome announces readiness on stderr as soon as the devtools server
        // is listening:
        //     DevTools listening on ws://127.0.0.1:<port>/devtools/browser/<id>
        // It also writes the chosen port into DevToolsActivePort under the
        // temporary profile dir. Poll both signals so launch does not depend
        // on one specific logging path.
        let stderr = child.stderr.take().expect("stderr is piped");
        let devtools_active_port_path = user_data_dir.join("DevToolsActivePort");

        let ws_url_result = tokio::time::timeout(config.timeout, async {
            let mut stderr_tail = VecDeque::with_capacity(Self::CHROME_STDERR_TAIL_LINES);
            let mut reader = BufReader::new(stderr).lines();
            let mut stderr_closed = false;

            loop {
                if let Some(url) =
                    Self::read_devtools_active_port_ws_url(&devtools_active_port_path)?
                {
                    tokio::spawn(async move {
                        let mut reader = reader;
                        while let Ok(Some(_)) = reader.next_line().await {}
                    });
                    return Ok::<String, BrowserError>(url);
                }

                if stderr_closed {
                    return Err(BrowserError::browser_not_launched(
                        BrowserLaunchErrorKind::DevtoolsPortNotAnnounced,
                        Self::format_chrome_launch_message(
                            "Chrome exited before announcing its DevTools port",
                            &stderr_tail,
                        ),
                    ));
                }

                tokio::select! {
                    line = reader.next_line() => match line.map_err(|e| {
                        BrowserError::browser_not_launched(
                            BrowserLaunchErrorKind::StderrReadFailed,
                            Self::format_chrome_launch_message(
                                format!("stderr read failed: {e}"),
                                &stderr_tail,
                            ),
                        )
                    })? {
                        Some(line) => {
                            Self::push_stderr_tail(&mut stderr_tail, line.clone());
                            const PREFIX: &str = "DevTools listening on ";
                            if let Some(idx) = line.find(PREFIX) {
                                let url = line[idx + PREFIX.len()..].trim().to_string();
                                tokio::spawn(async move {
                                    let mut reader = reader;
                                    while let Ok(Some(_)) = reader.next_line().await {}
                                });
                                return Ok::<String, BrowserError>(url);
                            }
                        }
                        None => stderr_closed = true,
                    },
                    _ = tokio::time::sleep(Self::CHROME_READY_POLL_INTERVAL) => {}
                }
            }
        })
        .instrument(tracing::info_span!("wait_for_chrome_ready"))
        .await;

        let ws_url = match ws_url_result {
            Ok(Ok(url)) => url,
            Ok(Err(err)) => {
                Self::cleanup_failed_launch(&mut child, Some(&user_data_dir)).await;
                return Err(err);
            }
            Err(_) => {
                Self::cleanup_failed_launch(&mut child, Some(&user_data_dir)).await;
                return Err(BrowserError::browser_not_launched(
                    BrowserLaunchErrorKind::StartupTimedOut,
                    format!("Chrome did not start within {}s", config.timeout.as_secs()),
                ));
            }
        };

        Ok(SpawnedChrome {
            child,
            pid,
            ws_url,
            user_data_dir,
        })
    }

    fn should_retry_launch(err: &BrowserError) -> bool {
        match err {
            BrowserError::BrowserNotLaunched { kind, .. } => kind.is_retryable(),
            _ => false,
        }
    }

    fn push_stderr_tail(stderr_tail: &mut VecDeque<String>, line: String) {
        if stderr_tail.len() == Self::CHROME_STDERR_TAIL_LINES {
            stderr_tail.pop_front();
        }
        stderr_tail.push_back(line);
    }

    fn format_chrome_launch_message(
        base: impl Into<Cow<'static, str>>,
        stderr_tail: &VecDeque<String>,
    ) -> Cow<'static, str> {
        let base = base.into();
        if stderr_tail.is_empty() {
            return base;
        }

        let mut message = base.into_owned();
        message.push_str("; chrome stderr tail: ");
        for (idx, line) in stderr_tail.iter().enumerate() {
            if idx > 0 {
                message.push_str(" | ");
            }
            message.push_str(line);
        }
        Cow::Owned(message)
    }

    async fn cleanup_failed_launch(
        child: &mut tokio::process::Child,
        user_data_dir: Option<&Path>,
    ) {
        let _ = child.start_kill();
        let _ = child.wait().await;
        if let Some(path) = user_data_dir {
            Self::cleanup_user_data_dir(path);
        }
    }

    fn create_chrome_user_data_dir() -> Result<PathBuf> {
        static CHROME_PROFILE_COUNTER: AtomicU64 = AtomicU64::new(0);

        let unique = CHROME_PROFILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ferrous-browser-profile-{}-{timestamp}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|err| {
            BrowserError::browser_not_launched(
                BrowserLaunchErrorKind::SpawnFailed,
                format!(
                    "Failed to create temporary Chrome user data dir '{}': {err}",
                    path.display()
                ),
            )
        })?;
        Ok(path)
    }

    fn read_devtools_active_port_ws_url(path: &Path) -> Result<Option<String>> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(BrowserError::browser_not_launched(
                    BrowserLaunchErrorKind::StderrReadFailed,
                    format!("failed to read {}: {err}", path.display()),
                ));
            }
        };

        let mut lines = contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty());
        let Some(port) = lines.next() else {
            return Ok(None);
        };
        let Some(endpoint) = lines.next() else {
            return Ok(None);
        };
        let Ok(port) = port.parse::<u16>() else {
            return Ok(None);
        };

        let ws_url = if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
            endpoint.to_string()
        } else if endpoint.starts_with('/') {
            format!("ws://127.0.0.1:{port}{endpoint}")
        } else {
            format!("ws://127.0.0.1:{port}/devtools/browser/{endpoint}")
        };
        Ok(Some(ws_url))
    }

    fn cleanup_user_data_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
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
        Self::connect_internal(ws_url, None, None).await
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
        Self::connect("ws://localhost:9222".to_owned()).await
    }

    async fn connect_internal(
        ws_url: String,
        pid: Option<Pid>,
        chrome_user_data_dir: Option<PathBuf>,
    ) -> Result<Self> {
        use futures_util::StreamExt;
        let cdp = Arc::new(CDPClient::new(ws_url));
        let ws_stream = cdp.connect().await?;
        let (sink, stream) = ws_stream.split();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        cdp.set_writer(tx);
        spawn_writer_task(sink, rx, cdp.clone());

        let conn = Connection::new(cdp.clone(), stream);
        tokio::spawn(conn.run());

        // Enable auto-attach so new targets connect instantly without round-trip
        cdp.send_command(
            "Target.setAutoAttach",
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
            chrome_user_data_dir,
        })
    }

    /// Create a new page/tab in the browser.
    #[tracing::instrument(level = "info", skip(self))]
    pub async fn new_page(&self) -> Result<Page> {
        // Subscribe to events BEFORE creating target so we don't miss attachedToTarget
        let mut event_rx = self.cdp.subscribe_events();

        let target_response = self
            .cdp
            .send_command("Target.createTarget", Some(json!({ "url": "about:blank" })))
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
        if let Some(path) = &self.chrome_user_data_dir {
            Self::cleanup_user_data_dir(path);
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
        assert_eq!(cfg.chrome_path, None);
        assert!(cfg.args.is_empty());
    }

    #[test]
    fn test_browser_config_custom() {
        let cfg = BrowserConfig {
            headless: false,
            timeout: Duration::from_secs(60),
            viewport: (1920, 1080),
            chrome_path: Some("/usr/bin/chromium".to_string()),
            args: vec!["--disable-extensions".to_string()],
        };
        assert!(!cfg.headless);
        assert_eq!(cfg.viewport, (1920, 1080));
        assert_eq!(cfg.timeout, Duration::from_secs(60));
        assert_eq!(cfg.chrome_path.as_deref(), Some("/usr/bin/chromium"));
        assert_eq!(cfg.args, vec!["--disable-extensions"]);
    }
}
