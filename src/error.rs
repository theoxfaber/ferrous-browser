use std::borrow::Cow;
use std::fmt;

use thiserror::Error;

/// Result type for ferrous-browser operations
pub type Result<T> = std::result::Result<T, BrowserError>;

/// Classification for Chrome launch/connect failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserLaunchErrorKind {
    /// The provided browser configuration was invalid.
    ConfigInvalid,
    /// Chrome/Chromium could not be found on the system.
    NotFound,
    /// Spawning the Chrome process failed.
    SpawnFailed,
    /// Chrome exited before reporting a process id.
    MissingPid,
    /// Reading Chrome's readiness signal from stderr failed.
    StderrReadFailed,
    /// Chrome exited before announcing its DevTools websocket URL.
    DevtoolsPortNotAnnounced,
    /// Chrome did not announce readiness before the configured timeout.
    StartupTimedOut,
    /// Chrome launched but the websocket connection step failed.
    ConnectFailed,
    /// Retrying exhausted the configured launch attempts.
    RetriesExhausted,
}

impl BrowserLaunchErrorKind {
    /// Whether a launch failure of this kind is worth retrying.
    pub const fn is_retryable(self) -> bool {
        matches!(
            self,
            BrowserLaunchErrorKind::StderrReadFailed
                | BrowserLaunchErrorKind::DevtoolsPortNotAnnounced
                | BrowserLaunchErrorKind::StartupTimedOut
                | BrowserLaunchErrorKind::ConnectFailed
        )
    }
}

impl fmt::Display for BrowserLaunchErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            BrowserLaunchErrorKind::ConfigInvalid => "config invalid",
            BrowserLaunchErrorKind::NotFound => "not found",
            BrowserLaunchErrorKind::SpawnFailed => "spawn failed",
            BrowserLaunchErrorKind::MissingPid => "missing pid",
            BrowserLaunchErrorKind::StderrReadFailed => "stderr read failed",
            BrowserLaunchErrorKind::DevtoolsPortNotAnnounced => "devtools port not announced",
            BrowserLaunchErrorKind::StartupTimedOut => "startup timed out",
            BrowserLaunchErrorKind::ConnectFailed => "connect failed",
            BrowserLaunchErrorKind::RetriesExhausted => "retries exhausted",
        };
        f.write_str(label)
    }
}

/// Classification for page helper failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageHelperErrorKind {
    /// The helper timed out before the condition was met.
    TimedOut,
    /// The helper threw a JavaScript exception while executing in-page.
    JavaScriptException,
    /// The helper response was missing required data.
    MissingPayload,
    /// The helper response payload was structurally invalid.
    InvalidPayload,
}

impl fmt::Display for PageHelperErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            PageHelperErrorKind::TimedOut => "timed out",
            PageHelperErrorKind::JavaScriptException => "javascript exception",
            PageHelperErrorKind::MissingPayload => "missing payload",
            PageHelperErrorKind::InvalidPayload => "invalid payload",
        };
        f.write_str(label)
    }
}

/// Errors that can occur during browser automation
#[derive(Error, Debug)]
pub enum BrowserError {
    /// WebSocket protocol error
    #[error("WebSocket error during {operation}: {message}")]
    WebSocket {
        /// The operation that was being performed
        operation: Cow<'static, str>,
        /// The underlying error message
        message: Cow<'static, str>,
    },

    /// Failed to establish initial connection
    #[error("Failed to connect to '{endpoint}': {reason}")]
    ConnectionFailed {
        /// The endpoint being connected to
        endpoint: Cow<'static, str>,
        /// The reason for failure
        reason: Cow<'static, str>,
    },

    /// Invalid or malformed CDP response
    #[error("Invalid CDP response while {context}: {details}")]
    InvalidResponse {
        /// What was being done
        context: Cow<'static, str>,
        /// Specific problem with the response
        details: Cow<'static, str>,
    },

    /// CDP command execution failed
    #[error("Command '{command}' failed: {reason}")]
    CommandFailed {
        /// The CDP command that failed
        command: Cow<'static, str>,
        /// The reason for failure
        reason: Cow<'static, str>,
    },

    /// CDP protocol error with code
    #[error("CDP error {code} in '{method}': {message}")]
    CdpError {
        /// CDP error code
        code: i32,
        /// The method that returned the error
        method: Cow<'static, str>,
        /// Error message
        message: Cow<'static, str>,
    },

    /// Operation exceeded timeout duration
    #[error("Timed out {operation} after {timeout_secs}s")]
    Timeout {
        /// Description of the operation that timed out
        operation: Cow<'static, str>,
        /// Timeout duration in seconds
        timeout_secs: u64,
    },

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// IO error from standard library
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Requested page was not found
    #[error("Page not found: {0}")]
    PageNotFound(Cow<'static, str>),

    /// Requested target was not found
    #[error("Target not found: {0}")]
    TargetNotFound(Cow<'static, str>),

    /// Browser instance not yet launched
    #[error("Browser not launched ({kind}): {message}")]
    BrowserNotLaunched {
        /// Typed launch/connect failure classification.
        kind: BrowserLaunchErrorKind,
        /// Human-readable launch/connect details.
        message: Cow<'static, str>,
    },

    /// A page helper returned a typed failure.
    #[error("Page helper '{helper}' {kind}: {details}")]
    PageHelperFailure {
        /// Helper name.
        helper: &'static str,
        /// Typed helper failure classification.
        kind: PageHelperErrorKind,
        /// Human-readable details for logging/debugging.
        details: Cow<'static, str>,
    },

    /// Navigation failed
    #[error("Navigation to '{url}' failed: {reason}")]
    NavigationFailed {
        /// The URL that failed to load
        url: Cow<'static, str>,
        /// The reason for failure (e.g. net::ERR_NAME_NOT_RESOLVED)
        reason: Cow<'static, str>,
    },
}

impl BrowserError {
    /// Construct a WebSocket error
    pub fn websocket(
        operation: impl Into<Cow<'static, str>>,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::WebSocket {
            operation: operation.into(),
            message: message.into(),
        }
    }

    /// Construct a connection-failed error
    pub fn connection_failed(
        endpoint: impl Into<Cow<'static, str>>,
        reason: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::ConnectionFailed {
            endpoint: endpoint.into(),
            reason: reason.into(),
        }
    }

    /// Construct a command-failed error
    pub fn command_failed(
        command: impl Into<Cow<'static, str>>,
        reason: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::CommandFailed {
            command: command.into(),
            reason: reason.into(),
        }
    }

    /// Construct an invalid-response error
    pub fn invalid_response(
        context: impl Into<Cow<'static, str>>,
        details: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::InvalidResponse {
            context: context.into(),
            details: details.into(),
        }
    }

    /// Construct a timeout error
    pub fn timeout(operation: impl Into<Cow<'static, str>>, timeout_secs: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_secs,
        }
    }

    /// Construct a typed browser-launch error.
    pub fn browser_not_launched(
        kind: BrowserLaunchErrorKind,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::BrowserNotLaunched {
            kind,
            message: message.into(),
        }
    }

    /// Construct a typed page-helper failure.
    pub fn page_helper(
        helper: &'static str,
        kind: PageHelperErrorKind,
        details: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::PageHelperFailure {
            helper,
            kind,
            details: details.into(),
        }
    }

    /// Construct a navigation-failed error
    pub fn navigation_failed(
        url: impl Into<Cow<'static, str>>,
        reason: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self::NavigationFailed {
            url: url.into(),
            reason: reason.into(),
        }
    }
}

/// Extension trait to add `.context(msg)` to `Result<T, BrowserError>`
pub trait ResultExt<T> {
    /// Add context to an error, wrapping it in a `CommandFailed` variant
    fn context(self, ctx: impl Into<Cow<'static, str>>) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, ctx: impl Into<Cow<'static, str>>) -> Result<T> {
        self.map_err(|e| BrowserError::CommandFailed {
            command: ctx.into(),
            reason: e.to_string().into(),
        })
    }
}
