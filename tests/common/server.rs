// Local HTTP test fixture for composite_idle Tier 2 tests.
//
// `TestServer::spawn()` binds on 127.0.0.1:0, returns its actual SocketAddr,
// and serves the routes documented below until dropped. Tests build URLs
// from `server.url("/path")` and point Chrome at them.
//
// Routes:
//   GET /static                  → 200 small body
//   GET /slow?ms=N               → sleep N ms, then 200
//   GET /chunked?n=K&gap=M       → K chunks with M ms between them
//   GET /stall                   → 200 headers, body hangs forever
//   GET /redirect?to=URL&status=N→ N redirect to URL (default 302)
//   GET /error?status=N          → return HTTP status N with small body
//   GET /ws                      → WebSocket upgrade; echoes one frame then idles
//   GET /sse                     → text/event-stream; sends one event then idles

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::extract::Request;
use axum::extract::{ws::WebSocketUpgrade, Query};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{stream, StreamExt};
use serde::Deserialize;
use tokio::sync::oneshot;

pub struct TestServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TestServer {
    pub async fn spawn() -> Self {
        let app = Router::new()
            .route("/static", get(handle_static))
            .route("/page", get(handle_page))
            .route("/slow", get(handle_slow))
            .route("/chunked", get(handle_chunked))
            .route("/stall", get(handle_stall))
            .route("/redirect", get(handle_redirect))
            .route("/error", get(handle_error))
            .route("/ws", get(handle_ws))
            .route("/sse", get(handle_sse))
            .layer(middleware::from_fn(add_cors_headers));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind 127.0.0.1:0");
        let addr = listener.local_addr().expect("local_addr");

        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });

        TestServer {
            addr,
            shutdown: Some(tx),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

// ─── handlers ───────────────────────────────────────────────────────────────

async fn handle_static() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        "<!doctype html><html><body>static</body></html>",
    )
}

#[derive(Deserialize)]
struct PageQ {
    html: String,
}
/// Serves arbitrary HTML — lets tests host the *page itself* on the same
/// origin as the fetch endpoints, sidestepping the null-origin CORS issues
/// that `data:` URLs trigger for things like WebSocket and same-origin
/// fetches.
async fn handle_page(Query(q): Query<PageQ>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        q.html,
    )
}

async fn add_cors_headers(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        "GET, POST, OPTIONS".parse().unwrap(),
    );
    h.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, "*".parse().unwrap());
    resp
}

#[derive(Deserialize)]
struct SlowQ {
    ms: Option<u64>,
}
async fn handle_slow(Query(q): Query<SlowQ>) -> impl IntoResponse {
    let ms = q.ms.unwrap_or(100);
    tokio::time::sleep(Duration::from_millis(ms)).await;
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain")],
        format!("slow ({} ms)", ms),
    )
}

#[derive(Deserialize)]
struct ChunkedQ {
    n: Option<usize>,
    gap: Option<u64>,
}
async fn handle_chunked(Query(q): Query<ChunkedQ>) -> impl IntoResponse {
    let n = q.n.unwrap_or(5);
    let gap = q.gap.unwrap_or(50);
    let chunks = (0..n).map(move |i| {
        let body = format!("chunk-{}\n", i);
        async move {
            if i > 0 {
                tokio::time::sleep(Duration::from_millis(gap)).await;
            }
            Ok::<_, Infallible>(body.into_bytes())
        }
    });
    // axum Body::from_stream takes a Stream<Item = Result<Bytes, _>>.
    let stream = stream::iter(chunks).then(|fut| fut);
    let body = Body::from_stream(stream);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(body)
        .unwrap()
}

async fn handle_stall() -> impl IntoResponse {
    // Send headers then never produce a body. We model this by streaming a
    // future that simply never resolves.
    let stream = stream::once(async {
        std::future::pending::<()>().await;
        Ok::<Vec<u8>, Infallible>(Vec::new())
    });
    let body = Body::from_stream(stream);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(body)
        .unwrap()
}

#[derive(Deserialize)]
struct RedirectQ {
    to: String,
    status: Option<u16>,
}
async fn handle_redirect(Query(q): Query<RedirectQ>) -> Response {
    let status = q.status.unwrap_or(302);
    if status == 301 {
        Redirect::permanent(&q.to).into_response()
    } else {
        Redirect::temporary(&q.to).into_response()
    }
}

#[derive(Deserialize)]
struct ErrorQ {
    status: Option<u16>,
}
async fn handle_error(Query(q): Query<ErrorQ>) -> Response {
    let code = q.status.unwrap_or(500);
    let st = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (st, format!("error {}", code)).into_response()
}

async fn handle_ws(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(|mut socket| async move {
        // Echo one frame then sit idle until the client (or shutdown) closes.
        use axum::extract::ws::Message;
        if let Some(Ok(Message::Text(t))) = socket.recv().await {
            let _ = socket.send(Message::Text(t)).await;
        }
        // Hold the connection open.
        std::future::pending::<()>().await;
    })
}

async fn handle_sse() -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    // Send one event immediately, then hold the connection open.
    let s = stream::iter(vec![Ok(SseEvent::default().data("hello"))]).chain(stream::once(async {
        std::future::pending::<Result<SseEvent, Infallible>>().await
    }));
    Sse::new(s)
}

// ── tiny helper for tests that want to drop a header map quickly ─────────
#[allow(dead_code)]
pub fn no_cache_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    h
}
