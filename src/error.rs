use thiserror::Error;

/// Result type for ferrous-browser operations
pub type Result<T> = std::result::Result<T, BrowserError>;

/// Errors that can occur during browser automation
#[derive(Error, Debug)]
pub enum BrowserError {
    /// WebSocket protocol error
    #[error("WebSocket error during {operation}: {message}")]
    WebSocket {
        /// The operation that was being performed
        operation: String,
        /// The underlying error message
        message: String,
    },

    /// Failed to establish initial connection
    #[error("Failed to connect to '{endpoint}': {reason}")]
    ConnectionFailed {
        /// The endpoint being connected to
        endpoint: String,
        /// The reason for failure
        reason: String,
    },

    /// Invalid or malformed CDP response
    #[error("Invalid CDP response while {context}: {details}")]
    InvalidResponse {
        /// What was being done
        context: String,
        /// Specific problem with the response
        details: String,
    },

    /// CDP command execution failed
    #[error("Command '{command}' failed: {reason}")]
    CommandFailed {
        /// The CDP command that failed
        command: String,
        /// The reason for failure
        reason: String,
    },

    /// CDP protocol error with code
    #[error("CDP error {code} in '{method}': {message}")]
    CdpError {
        /// CDP error code
        code: i32,
        /// The method that returned the error
        method: String,
        /// Error message
        message: String,
    },

    /// Operation exceeded timeout duration
    #[error("Timed out {operation} after {timeout_secs}s")]
    Timeout {
        /// Description of the operation that timed out
        operation: String,
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
    PageNotFound(String),

    /// Requested target was not found
    #[error("Target not found: {0}")]
    TargetNotFound(String),

    /// Browser instance not yet launched
    #[error("Browser not launched: {0}")]
    BrowserNotLaunched(String),

    /// Navigation failed
    #[error("Navigation to '{url}' failed: {reason}")]
    NavigationFailed {
        /// The URL that failed to load
        url: String,
        /// The reason for failure (e.g. net::ERR_NAME_NOT_RESOLVED)
        reason: String,
    },
}

impl BrowserError {
    /// Construct a WebSocket error
    pub fn websocket(operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self::WebSocket {
            operation: operation.into(),
            message: message.into(),
        }
    }

    /// Construct a connection-failed error
    pub fn connection_failed(endpoint: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ConnectionFailed {
            endpoint: endpoint.into(),
            reason: reason.into(),
        }
    }

    /// Construct a command-failed error
    pub fn command_failed(command: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::CommandFailed {
            command: command.into(),
            reason: reason.into(),
        }
    }

    /// Construct an invalid-response error
    pub fn invalid_response(context: impl Into<String>, details: impl Into<String>) -> Self {
        Self::InvalidResponse {
            context: context.into(),
            details: details.into(),
        }
    }

    /// Construct a timeout error
    pub fn timeout(operation: impl Into<String>, timeout_secs: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_secs,
        }
    }

    /// Construct a navigation-failed error
    pub fn navigation_failed(url: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::NavigationFailed {
            url: url.into(),
            reason: reason.into(),
        }
    }
}

/// Extension trait to add `.context(msg)` to `Result<T, BrowserError>`
pub trait ResultExt<T> {
    /// Add context to an error, wrapping it in a `CommandFailed` variant
    fn context(self, ctx: impl Into<String>) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, ctx: impl Into<String>) -> Result<T> {
        self.map_err(|e| BrowserError::CommandFailed {
            command: ctx.into(),
            reason: e.to_string(),
        })
    }
}
