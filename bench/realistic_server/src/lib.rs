use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::{Path, Query};
use axum::http::header;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use tokio::sync::oneshot;

const INDEX_HTML: &str = include_str!("../../realistic/fixtures/signalboard/index.html");
const APP_CSS: &str = include_str!("../../realistic/fixtures/signalboard/app.css");

pub struct SignalboardServer {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl SignalboardServer {
    pub async fn spawn() -> Self {
        let app = Router::new()
            .route("/signalboard", get(|| async { Redirect::temporary("/signalboard/") }))
            .route("/signalboard/", get(index))
            .route("/signalboard/app.css", get(app_css))
            .route("/signalboard/api/bootstrap", get(api_bootstrap))
            .route("/signalboard/api/cards", get(api_cards))
            .route("/signalboard/api/alerts", get(api_alerts))
            .route("/signalboard/api/activity", get(api_activity))
            .route("/signalboard/api/insights", get(api_insights))
            .route("/signalboard/api/prefetch", get(api_prefetch))
            .route("/signalboard/api/detail", get(api_detail))
            .route("/signalboard/api/detail-audit", get(api_detail_audit))
            .route("/signalboard/assets/:name", get(asset))
            .layer(middleware::from_fn(add_no_store_headers));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind signalboard server");
        let addr = listener.local_addr().expect("signalboard local_addr");

        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });

        Self {
            addr,
            shutdown: Some(tx),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}/signalboard/", self.addr)
    }
}

impl Drop for SignalboardServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

async fn add_no_store_headers(req: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store, no-cache, must-revalidate".parse().unwrap());
    headers.insert(header::PRAGMA, "no-cache".parse().unwrap());
    headers.insert(header::EXPIRES, "0".parse().unwrap());
    response
}

async fn index() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
}

async fn app_css() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8")], APP_CSS)
}

async fn api_bootstrap() -> impl IntoResponse {
    sleep_ms(110).await;
    Json(serde_json::json!({
        "heroTitle": "Regional Service Health",
        "heroSummary": "Balancing fan-out, cache pressure, and render latency through the morning traffic ramp.",
        "kpis": [
            {"label": "Regions", "value": "6"},
            {"label": "Queues", "value": "14"},
            {"label": "Edge", "value": "98.7%"},
        ],
    }))
}

async fn api_cards() -> impl IntoResponse {
    sleep_ms(180).await;
    Json(serde_json::json!({
        "cards": [
            {
                "id": "latency-lab",
                "title": "Latency Lab",
                "status": "Watching",
                "delta": "+18 ms",
                "summary": "A slow regional fan-out is stretching the render queue after cache misses.",
                "cta": "Open detail",
            },
            {
                "id": "cdn-pulse",
                "title": "CDN Pulse",
                "status": "Stable",
                "delta": "-4 ms",
                "summary": "Thumbnail propagation recovered after the overnight purge window.",
                "cta": "Inspect",
            },
            {
                "id": "queue-watch",
                "title": "Queue Watch",
                "status": "Holding",
                "delta": "+3 jobs",
                "summary": "Consumer lag is contained, but worker saturation is edging toward the guardrail.",
                "cta": "Inspect",
            }
        ]
    }))
}

async fn api_alerts() -> impl IntoResponse {
    sleep_ms(260).await;
    Json(serde_json::json!({
        "alerts": [
            {
                "title": "Retry surge",
                "summary": "Cross-region retries lifted 9% after the east cache rewarm.",
            },
            {
                "title": "Thumbnail backlog",
                "summary": "Hero image transforms are draining, but the second wave is still en route.",
            }
        ]
    }))
}

async fn api_activity() -> impl IntoResponse {
    sleep_ms(420).await;
    Json(serde_json::json!({
        "activity": [
            {
                "title": "Capture lane",
                "summary": "Fresh telemetry is landing on the fast path with only one delayed shard.",
            },
            {
                "title": "Fan-out graph",
                "summary": "The replica spread widened by two regions while the edge rebuilt.",
            },
            {
                "title": "Render queue",
                "summary": "Renderer saturation rose after the batch replay and is easing slowly.",
            },
            {
                "title": "Edge cache",
                "summary": "The top route recovered, but the warmup traffic is still in flight.",
            }
        ]
    }))
}

async fn api_insights() -> impl IntoResponse {
    sleep_ms(1400).await;
    Json(serde_json::json!({ "complete": true }))
}

async fn api_prefetch() -> impl IntoResponse {
    sleep_ms(1800).await;
    Json(serde_json::json!({ "complete": true }))
}

#[derive(Deserialize)]
struct DetailQuery {
    id: Option<String>,
}

async fn api_detail(Query(query): Query<DetailQuery>) -> impl IntoResponse {
    sleep_ms(260).await;
    Json(serde_json::json!({
        "id": query.id.unwrap_or_else(|| "latency-lab".to_string()),
        "title": "Latency Lab",
        "owner": "Runtime Operations",
        "summary": "The render queue is waiting on a slow regional response burst. User-visible controls are ready well before the background audit drains.",
        "stages": ["Capture", "Aggregate", "Render"],
        "auditWindow": "Background audit closes after the next fan-out sample.",
    }))
}

async fn api_detail_audit() -> impl IntoResponse {
    sleep_ms(900).await;
    Json(serde_json::json!({ "complete": true }))
}

async fn asset(Path(name): Path<String>) -> impl IntoResponse {
    let asset = match name.as_str() {
        "hero-east.svg" => Some(("East fan-out", "#0f6f6a", "#4aa89f", 480_u64)),
        "hero-west.svg" => Some(("West queue", "#9e4f24", "#d6a04e", 620_u64)),
        "detail-chart.svg" => Some(("Audit trace", "#163a55", "#3b7ba8", 380_u64)),
        _ => None,
    };

    match asset {
        Some((title, background, accent, delay_ms)) => {
            sleep_ms(delay_ms).await;
            (
                [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
                svg_markup(title, background, accent),
            )
                .into_response()
        }
        None => Response::builder()
            .status(404)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body("not found".into())
            .unwrap(),
    }
}

async fn sleep_ms(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

fn svg_markup(title: &str, background: &str, accent: &str) -> String {
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 720 480">"#,
            r#"<defs><linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">"#,
            r#"<stop offset="0%" stop-color="{background}" />"#,
            r#"<stop offset="100%" stop-color="{accent}" />"#,
            r#"</linearGradient></defs>"#,
            r#"<rect width="720" height="480" fill="url(#bg)" rx="36" ry="36" />"#,
            r#"<circle cx="118" cy="114" r="56" fill="rgba(255,255,255,0.18)" />"#,
            r#"<path d="M72 352C152 284 248 302 330 248C382 214 450 146 560 164C612 174 650 198 682 224V480H38V396C46 380 58 364 72 352Z" fill="rgba(255,255,255,0.14)" />"#,
            r#"<path d="M98 288L188 244L262 268L360 196L454 228L530 178L630 212" fill="none" stroke="rgba(255,255,255,0.9)" stroke-width="14" stroke-linecap="round" stroke-linejoin="round" />"#,
            r#"<text x="52" y="420" fill="white" font-family="IBM Plex Sans, Segoe UI, sans-serif" font-size="44" font-weight="700">{title}</text>"#,
            r#"</svg>"#
        ),
        background = background,
        accent = accent,
        title = title
    )
}
