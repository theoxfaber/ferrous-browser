use criterion::{criterion_group, criterion_main, Criterion};
use ferrous_browser::{Browser, WaitUntil};
use tokio::runtime::Runtime;

/// Run with: cargo bench
/// Requires: Chrome on localhost:9222 (or launch_chrome will start one)
fn bench_connect(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    c.bench_function("connect_to_chrome", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = Browser::launch().await;
        })
    });
}

fn bench_new_page(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let browser = rt.block_on(async { Browser::launch().await.ok() });
    let Some(browser) = browser else {
        eprintln!("Chrome not running, skipping bench_new_page");
        return;
    };
    c.bench_function("new_page", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = browser.new_page().await;
        })
    });
}

fn bench_navigate_content(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let setup = rt.block_on(async {
        let b = Browser::launch().await.ok()?;
        let p = b.new_page().await.ok()?;
        Some((b, p))
    });
    let Some((_browser, page)) = setup else {
        eprintln!("Chrome not running, skipping bench_navigate_content");
        return;
    };
    c.bench_function("navigate_and_content", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = page.goto("https://example.com", WaitUntil::Load).await;
            let _ = page.content().await;
        })
    });
}

fn bench_screenshot(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let setup = rt.block_on(async {
        let b = Browser::launch().await.ok()?;
        let p = b.new_page().await.ok()?;
        p.goto("https://example.com", WaitUntil::Load).await.ok()?;
        Some((b, p))
    });
    let Some((_browser, page)) = setup else {
        eprintln!("Chrome not running, skipping bench_screenshot");
        return;
    };
    c.bench_function("screenshot_png", |b| {
        b.to_async(&rt).iter(|| async {
            let _ = page.screenshot().await;
        })
    });
}

fn bench_evaluate(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let setup = rt.block_on(async {
        let b = Browser::launch().await.ok()?;
        let p = b.new_page().await.ok()?;
        p.goto("https://example.com", WaitUntil::Load).await.ok()?;
        Some((b, p))
    });
    let Some((_browser, page)) = setup else {
        eprintln!("Chrome not running, skipping bench_evaluate");
        return;
    };
    c.bench_function("evaluate_js", |b| {
        b.to_async(&rt).iter(|| async {
            let _: Result<String, _> = page.evaluate("document.title").await;
        })
    });
}

criterion_group!(
    benches,
    bench_connect,
    bench_new_page,
    bench_navigate_content,
    bench_screenshot,
    bench_evaluate,
);
criterion_main!(benches);

