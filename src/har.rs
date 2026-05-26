use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::cdp::CDPClient;
use crate::error::Result;

/// A single HTTP Archive (HAR) entry representing a network request/response pair.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarEntry {
    /// Unique page reference within the HAR
    pub pageref: String,
    /// ISO 8601 timestamp when the request started
    pub started_date_time: String,
    /// Total elapsed time in milliseconds
    pub time: f64,
    /// Request details
    pub request: HarRequest,
    /// Response details
    pub response: HarResponse,
    /// Cache state
    pub cache: Value,
    /// Timing breakdown
    pub timings: HarTimings,
    /// Server IP address (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_ip_address: Option<String>,
    /// Connection UUID (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
}

/// HAR request object
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarRequest {
    /// HTTP method
    pub method: String,
    /// Request URL
    pub url: String,
    /// HTTP version
    pub http_version: String,
    /// Request headers as name-value pairs
    pub headers: Vec<HarHeader>,
    /// Query string parameters
    pub query_string: Vec<HarQueryParam>,
    /// Size of request headers in bytes (-1 if unknown)
    pub headers_size: i64,
    /// Size of request body in bytes (-1 if unknown)
    pub body_size: i64,
    /// POST data (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_data: Option<HarPostData>,
}

/// HAR response object
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarResponse {
    /// HTTP status code
    pub status: i64,
    /// HTTP status text
    pub status_text: String,
    /// HTTP version
    pub http_version: String,
    /// Response headers as name-value pairs
    pub headers: Vec<HarHeader>,
    /// Response cookies
    pub cookies: Vec<HarCookie>,
    /// Response content metadata
    pub content: HarContent,
    /// Redirect URL (empty if no redirect)
    pub redirect_url: String,
    /// Size of response headers in bytes (-1 if unknown)
    pub headers_size: i64,
    /// Size of response body in bytes (-1 if unknown)
    pub body_size: i64,
}

/// HAR timing object (all values in milliseconds)
#[derive(Debug, Clone, Serialize)]
pub struct HarTimings {
    /// Time spent in DNS lookup
    pub dns: f64,
    /// Time spent in TCP connection
    pub connect: f64,
    /// Time spent in TLS handshake
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssl: Option<f64>,
    /// Time spent sending request
    pub send: f64,
    /// Time spent waiting for response
    pub wait: f64,
    /// Time spent receiving response
    pub receive: f64,
    /// Total blocked time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<f64>,
}

/// HAR header name-value pair
#[derive(Debug, Clone, Serialize)]
pub struct HarHeader {
    /// Header name
    pub name: String,
    /// Header value
    pub value: String,
}

/// HAR query string parameter
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarQueryParam {
    /// Parameter name
    pub name: String,
    /// Parameter value
    pub value: String,
}

/// HAR POST data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarPostData {
    /// MIME type of the POST data
    pub mime_type: String,
    /// POST body text
    pub text: String,
}

/// HAR cookie
#[derive(Debug, Clone, Serialize)]
pub struct HarCookie {
    /// Cookie name
    pub name: String,
    /// Cookie value
    pub value: String,
    /// Cookie path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Cookie domain
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Expiry timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    /// Whether cookie is HTTP-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    /// Whether cookie is secure-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
}

/// HAR content metadata
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarContent {
    /// Size of the response body in bytes
    pub size: i64,
    /// MIME type of the response
    pub mime_type: String,
    /// Optional compression saving
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<i64>,
}

/// Full HAR log structure
#[derive(Debug, Clone, Serialize)]
pub struct HarLog {
    /// HAR spec version (e.g. "1.2")
    pub version: String,
    /// Tool that created the HAR
    pub creator: Value,
    /// Array of captured request/response entries
    pub entries: Vec<HarEntry>,
}

/// Top-level HAR structure matching the HTTP Archive spec
#[derive(Debug, Clone, Serialize)]
pub struct HarArchive {
    /// The log container
    pub log: HarLog,
}

// Internal state for building HAR entries from CDP network events.
// We track pending requests by requestId until they complete or fail.
#[derive(Debug, Clone)]
struct PendingRequest {
    method: String,
    url: String,
    http_version: String,
    request_headers: Vec<HarHeader>,
    query_string: Vec<HarQueryParam>,
    post_data: Option<HarPostData>,
    request_timestamp: f64,
    status: Option<i64>,
    status_text: Option<String>,
    response_headers: Vec<HarHeader>,
    response_cookies: Vec<HarCookie>,
    mime_type: Option<String>,
    redirect_url: String,
    headers_size: i64,
    body_size: i64,
    server_ip_address: Option<String>,
    connection: Option<String>,
    timing_dns: f64,
    timing_connect: f64,
    timing_ssl: Option<f64>,
    timing_send: f64,
    timing_wait: f64,
    timing_receive: f64,
    timing_blocked: Option<f64>,
}

impl PendingRequest {
    fn new(method: String, url: String, request_timestamp: f64) -> Self {
        Self {
            method,
            url,
            http_version: String::new(),
            request_headers: Vec::new(),
            query_string: Vec::new(),
            post_data: None,
            request_timestamp,
            status: None,
            status_text: None,
            response_headers: Vec::new(),
            response_cookies: Vec::new(),
            mime_type: None,
            redirect_url: String::new(),
            headers_size: -1,
            body_size: -1,
            server_ip_address: None,
            connection: None,
            timing_dns: -1.0,
            timing_connect: -1.0,
            timing_ssl: None,
            timing_send: -1.0,
            timing_wait: -1.0,
            timing_receive: -1.0,
            timing_blocked: None,
        }
    }

    fn finish(self, response_timestamp: f64) -> HarEntry {
        let time_ms = ((response_timestamp - self.request_timestamp) * 1000.0).max(0.0);
        let iso = iso_timestamp(self.request_timestamp);

        let http_version = self.http_version;
        let method = self.method;
        let url = self.url;
        let request_headers = self.request_headers;
        let query_string = self.query_string;
        let headers_size = self.headers_size;
        let body_size = self.body_size;
        let post_data = self.post_data;
        let status = self.status.unwrap_or(0);
        let status_text = self.status_text.unwrap_or_default();
        let response_headers = self.response_headers;
        let response_cookies = self.response_cookies;
        let mime_type = self.mime_type.unwrap_or_default();
        let redirect_url = self.redirect_url;
        let server_ip_address = self.server_ip_address;
        let connection = self.connection;
        let timing_dns = self.timing_dns;
        let timing_connect = self.timing_connect;
        let timing_ssl = self.timing_ssl;
        let timing_send = self.timing_send;
        let timing_wait = self.timing_wait;
        let timing_receive = self.timing_receive;
        let timing_blocked = self.timing_blocked;

        HarEntry {
            pageref: "page_1".to_string(),
            started_date_time: iso,
            time: time_ms,
            request: HarRequest {
                method,
                url,
                http_version: http_version.clone(),
                headers: request_headers,
                query_string,
                headers_size,
                body_size,
                post_data,
            },
            response: HarResponse {
                status,
                status_text,
                http_version,
                headers: response_headers,
                cookies: response_cookies,
                content: HarContent {
                    size: body_size.max(0),
                    mime_type,
                    compression: None,
                },
                redirect_url,
                headers_size,
                body_size,
            },
            cache: json!({}),
            timings: HarTimings {
                dns: timing_dns,
                connect: timing_connect,
                ssl: timing_ssl,
                send: timing_send,
                wait: timing_wait,
                receive: timing_receive,
                blocked: timing_blocked,
            },
            server_ip_address,
            connection,
        }
    }
}

struct CaptureState {
    pending: HashMap<String, PendingRequest>,
    entries: Vec<HarEntry>,
}

/// Captures HTTP Archive (HAR) data for a page by listening to Chrome's
/// Network domain events. Start it via [`crate::Page::start_har_capture`].
///
/// # Example
///
/// ```no_run
/// # use ferrous_browser::{Browser, WaitUntil};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let browser = Browser::launch_chrome(None).await?;
/// let page = browser.new_page().await?;
///
/// let har = page.start_har_capture().await?;
/// page.goto("https://example.com", WaitUntil::Load).await?;
///
/// let archive = har.export().await;
/// let json = serde_json::to_string_pretty(&archive)?;
/// std::fs::write("trace.har", json)?;
/// # Ok(())
/// # }
/// ```
pub struct HarCapture {
    cdp: Arc<CDPClient>,
    session_id: String,
    state: Arc<Mutex<CaptureState>>,
}

impl HarCapture {
    pub(crate) fn new(cdp: Arc<CDPClient>, session_id: String) -> Self {
        Self {
            cdp,
            session_id,
            state: Arc::new(Mutex::new(CaptureState {
                pending: HashMap::new(),
                entries: Vec::new(),
            })),
        }
    }

    /// Listen to CDP Network events and accumulate HAR entries.
    /// Call this to start capturing. Events are processed in a background
    /// task until `stop` or the `HarCapture` is dropped.
    pub async fn start(&self) -> Result<()> {
        // Enable Network domain — required for events
        self.cdp
            .send_command_with_session(&self.session_id, "Network.enable".to_string(), None)
            .await?;

        let mut rx = self.cdp.subscribe_events();
        let session_id = self.session_id.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(msg) if msg.session_id.as_deref() == Some(&session_id) => {
                        Self::handle_event(&state, msg.method.as_deref(), msg.params).await;
                    }
                    Ok(_) => {} // different session
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return, // channel closed
                }
            }
        });

        Ok(())
    }

    async fn handle_event(
        state: &Arc<Mutex<CaptureState>>,
        method: Option<&str>,
        params: Option<Value>,
    ) {
        let Some(params) = params else { return };
        match method {
            Some("Network.requestWillBeSent") => {
                Self::handle_request_will_be_sent(state, params).await;
            }
            Some("Network.responseReceived") => {
                Self::handle_response_received(state, params).await;
            }
            Some("Network.loadingFinished") => {
                Self::handle_loading_finished(state, params).await;
            }
            Some("Network.loadingFailed") => {
                Self::handle_loading_failed(state, params).await;
            }
            _ => {}
        }
    }

    async fn handle_request_will_be_sent(state: &Arc<Mutex<CaptureState>>, params: Value) {
        let request_id = params
            .get("requestId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let request = match params.get("request") {
            Some(r) => r,
            None => return,
        };
        let method = match request.get("method").and_then(|v| v.as_str()) {
            Some(m) => m.to_string(),
            None => return,
        };
        let url = match request.get("url").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => return,
        };
        let ts = params
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Determine HTTP version before moving `url`
        let http_version = {
            if let Some(ver) = request.get("headers").and_then(|h| h.get(":version")) {
                ver.as_str().unwrap_or("HTTP/2.0").to_string()
            } else if url.starts_with("https") {
                "HTTP/2.0".to_string()
            } else {
                "HTTP/1.1".to_string()
            }
        };

        let mut pending = PendingRequest::new(method, url, ts);
        pending.http_version = http_version;

        // Request headers
        if let Some(headers) = request.get("headers").and_then(|h| h.as_object()) {
            for (name, value) in headers {
                if let Some(val) = value.as_str() {
                    if !name.starts_with(':') {
                        pending.request_headers.push(HarHeader {
                            name: name.clone(),
                            value: val.to_string(),
                        });
                    }
                }
            }
        }

        // Query string
        if let Some(qs) = request.get("queryString").and_then(|v| v.as_array()) {
            for param in qs {
                if let (Some(name), Some(value)) = (
                    param.get("name").and_then(|v| v.as_str()),
                    param.get("value").and_then(|v| v.as_str()),
                ) {
                    pending.query_string.push(HarQueryParam {
                        name: name.to_string(),
                        value: value.to_string(),
                    });
                }
            }
        }

        // POST data
        if let Some(post) = request.get("postData") {
            if let (Some(text), mime) = (
                post.get("text").and_then(|v| v.as_str()),
                post.get("mimeType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("application/octet-stream"),
            ) {
                pending.post_data = Some(HarPostData {
                    mime_type: mime.to_string(),
                    text: text.to_string(),
                });
            }
        }

        let mut guard = state.lock().await;
        if let Some(id) = request_id {
            guard.pending.insert(id, pending);
        }
    }

    async fn handle_response_received(state: &Arc<Mutex<CaptureState>>, params: Value) {
        let request_id = params
            .get("requestId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let response = match params.get("response") {
            Some(r) => r,
            None => return,
        };

        let status = response.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
        let status_text = response
            .get("statusText")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mime_type = response
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let remote_ip = response
            .get("remoteIPAddress")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let connection_id = response
            .get("connectionId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Response headers
        let mut resp_headers = Vec::new();
        if let Some(headers) = response.get("headers").and_then(|h| h.as_object()) {
            for (name, value) in headers {
                if let Some(val) = value.as_str() {
                    if !name.starts_with(':') {
                        resp_headers.push(HarHeader {
                            name: name.clone(),
                            value: val.to_string(),
                        });
                    }
                }
            }
        }

        // Cookies
        let mut cookies = Vec::new();
        if let Some(cookie_array) = response.get("cookies").and_then(|c| c.as_array()) {
            for c in cookie_array {
                let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let value = c.get("value").and_then(|v| v.as_str()).unwrap_or("");
                cookies.push(HarCookie {
                    name: name.to_string(),
                    value: value.to_string(),
                    path: c
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    domain: c
                        .get("domain")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    expires: c
                        .get("expires")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    http_only: c.get("httpOnly").and_then(|v| v.as_bool()),
                    secure: c.get("secure").and_then(|v| v.as_bool()),
                });
            }
        }

        // Timing
        let timings = response.get("timing");
        let dns = timings
            .and_then(|t| t.get("dns"))
            .and_then(|v| v.as_f64())
            .unwrap_or(-1.0);
        let connect = timings
            .and_then(|t| t.get("connect"))
            .and_then(|v| v.as_f64())
            .unwrap_or(-1.0);
        let ssl = timings
            .and_then(|t| t.get("ssl"))
            .and_then(|v| v.as_f64())
            .filter(|&v| v >= 0.0);
        let send = timings
            .and_then(|t| t.get("send"))
            .and_then(|v| v.as_f64())
            .unwrap_or(-1.0);
        let wait = timings
            .and_then(|t| t.get("wait"))
            .and_then(|v| v.as_f64())
            .unwrap_or(-1.0);
        let receive = timings
            .and_then(|t| t.get("receive"))
            .and_then(|v| v.as_f64())
            .unwrap_or(-1.0);
        let blocked = timings
            .and_then(|t| t.get("blocked"))
            .and_then(|v| v.as_f64())
            .filter(|&v| v >= 0.0);

        // Redirect URL
        let redirect_url = response
            .get("redirectURL")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Headers size from response
        let headers_size = response
            .get("headersSize")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);

        let mut guard = state.lock().await;
        if let Some(ref id) = request_id {
            if let Some(p) = guard.pending.get_mut(id) {
                p.status = Some(status);
                p.status_text = Some(status_text);
                p.mime_type = Some(mime_type);
                p.server_ip_address = remote_ip;
                p.connection = connection_id;
                p.response_headers = resp_headers;
                p.response_cookies = cookies;
                p.timing_dns = dns;
                p.timing_connect = connect;
                p.timing_ssl = ssl;
                p.timing_send = send;
                p.timing_wait = wait;
                p.timing_receive = receive;
                p.timing_blocked = blocked;
                p.redirect_url = redirect_url;
                p.headers_size = headers_size;
            }
        }
    }

    async fn handle_loading_finished(state: &Arc<Mutex<CaptureState>>, params: Value) {
        let request_id = params
            .get("requestId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let ts = params
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let encoded_size = params
            .get("encodedDataLength")
            .and_then(|v| v.as_i64())
            .unwrap_or(-1);

        let mut guard = state.lock().await;
        if let Some(id) = request_id {
            if let Some(p) = guard.pending.remove(&id) {
                let mut entry = p.finish(ts);
                entry.response.content.size = encoded_size.max(0);
                entry.response.body_size = encoded_size;
                guard.entries.push(entry);
            }
        }
    }

    async fn handle_loading_failed(state: &Arc<Mutex<CaptureState>>, params: Value) {
        let request_id = params
            .get("requestId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let ts = params
            .get("timestamp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let mut guard = state.lock().await;
        if let Some(id) = request_id {
            if let Some(p) = guard.pending.remove(&id) {
                let mut entry = p.finish(ts);
                entry.response.status = 0;
                entry.response.status_text = params
                    .get("errorText")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Failed")
                    .to_string();
                guard.entries.push(entry);
            }
        }
    }

    /// Stop the capture and return the complete HAR archive.
    ///
    /// This drains any remaining pending requests (for requests that were
    /// sent but never received a response) and returns the full HAR.
    pub async fn stop(&self) -> HarArchive {
        let mut guard = self.state.lock().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        // Flush any dangling pending requests
        let mut all_entries = std::mem::take(&mut guard.entries);
        for (_, pending) in guard.pending.drain() {
            all_entries.push(pending.finish(now));
        }

        HarArchive {
            log: HarLog {
                version: "1.2".to_string(),
                creator: json!({
                    "name": "ferrous-browser",
                    "version": env!("CARGO_PKG_VERSION"),
                }),
                entries: all_entries,
            },
        }
    }

    /// Export the current HAR without stopping the capture.
    pub async fn export(&self) -> HarArchive {
        let guard = self.state.lock().await;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        let mut entries = guard.entries.clone();
        for (_, pending) in guard.pending.iter() {
            entries.push(pending.clone().finish(now));
        }

        HarArchive {
            log: HarLog {
                version: "1.2".to_string(),
                creator: json!({
                    "name": "ferrous-browser",
                    "version": env!("CARGO_PKG_VERSION"),
                }),
                entries,
            },
        }
    }

    /// Clear all captured entries and pending requests.
    pub async fn clear(&self) {
        let mut guard = self.state.lock().await;
        guard.pending.clear();
        guard.entries.clear();
    }
}

/// Convert a Unix timestamp (seconds since epoch) to ISO 8601 string.
fn iso_timestamp(ts: f64) -> String {
    let secs = ts as i64;
    let subsec_nanos = ((ts - secs as f64) * 1_000_000_000.0).round() as u32;

    // Decompose into year/month/day/hour/min/sec
    let days = secs / 86400;
    let time_secs = (secs % 86400).abs();
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Days since epoch to date (civil calendar)
    let (year, month, day) = days_to_date(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year,
        month,
        day,
        hours,
        minutes,
        seconds,
        subsec_nanos / 1_000_000
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(mut days: i64) -> (i64, u32, u32) {
    // Algorithm from Howard Hinnant
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}
