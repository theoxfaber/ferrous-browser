use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::{Page, ScreenshotParams};
use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const ITERS: usize = 10;
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const WAIT_NOTE: &str = "manual poll @50ms for ready/settled waits";
const CONDUIT_SLUG: &str = "composite-network-idle";
const CONDUIT_TITLE: &str = "Composite NetworkIdle";
const CONDUIT_COMMENT: &str = "Benchmark the real flow.";

#[derive(Clone)]
struct Stats {
    median: f64,
    p10: f64,
    n: usize,
    note: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    ready: bool,
    settled: bool,
    filter: String,
    total_count: usize,
    active_count: usize,
    completed_count: usize,
    visible_titles: Vec<String>,
    skeleton_visible: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConduitSnapshot {
    ready: bool,
    settled: bool,
    route: String,
    logged_in: bool,
    skeleton_visible: bool,
    login_visible: bool,
    feed_visible: bool,
    article_visible: bool,
    user_name: String,
    feed_titles: Vec<String>,
    composite_favorite_count: usize,
    composite_favorited: bool,
    selected_slug: Option<String>,
    article_title: Option<String>,
    article_ready: bool,
    article_tags: Vec<String>,
    article_comment_bodies: Vec<String>,
}

fn chrome_path() -> String {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let home = std::env::var("HOME").expect("HOME must be set or CHROME_PATH must be provided");
    format!("{home}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome")
}

fn todo_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../realistic/fixtures/todomvc/index.html")
        .canonicalize()
        .expect("canonicalize TodoMVC fixture");
    format!("file://{}", path.display())
}

fn conduit_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../realistic/fixtures/conduit/index.html")
        .canonicalize()
        .expect("canonicalize Conduit fixture");
    format!("file://{}", path.display())
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn p10(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[(xs.len() as f64 * 0.1) as usize]
}

fn stats(xs: Vec<f64>, note: Option<&'static str>) -> Stats {
    Stats {
        median: median(xs.clone()),
        p10: p10(xs),
        n: ITERS,
        note,
    }
}

fn print_stats(name: &str, s: &Stats) {
    match s.note {
        Some(note) => println!(
            "{name:28} median={:.2}ms p10={:.2}ms (n={}) [{note}]",
            s.median, s.p10, s.n
        ),
        None => println!(
            "{name:28} median={:.2}ms p10={:.2}ms (n={})",
            s.median, s.p10, s.n
        ),
    }
}

fn stats_to_json(s: &Stats) -> serde_json::Value {
    json!({
        "median": s.median,
        "p10": s.p10,
        "n": s.n,
        "note": s.note,
    })
}

fn expect_titles(
    actual: &[String],
    expected: &[&str],
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let expected_vec: Vec<String> = expected.iter().map(|value| value.to_string()).collect();
    if actual != expected_vec {
        return Err(format!("{label}: expected {:?}, got {:?}", expected_vec, actual).into());
    }
    Ok(())
}

fn assert_initial_snapshot(snapshot: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("initial snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.filter != "all"
        || snapshot.total_count != 3
        || snapshot.active_count != 2
        || snapshot.completed_count != 1
    {
        return Err(format!("unexpected initial counts: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.visible_titles,
        &[
            "Map realistic flows",
            "Ship stable waits",
            "Audit launch overhead",
        ],
        "initial visible titles",
    )
}

fn assert_completed_snapshot(snapshot: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
    if snapshot.filter != "completed" || snapshot.completed_count != 2 {
        return Err(format!("unexpected completed snapshot: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.visible_titles,
        &["Ship stable waits", "Trim flaky setup"],
        "completed visible titles",
    )
}

fn assert_active_filtered_snapshot(snapshot: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("active-filter snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.filter != "active"
        || snapshot.total_count != 5
        || snapshot.active_count != 3
        || snapshot.completed_count != 2
    {
        return Err(format!("unexpected active-filter counts: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.visible_titles,
        &[
            "Map realistic flows",
            "Audit launch overhead",
            "Capture settled screenshot",
        ],
        "active-filter visible titles",
    )
}

fn assert_final_snapshot(snapshot: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("final snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.filter != "all"
        || snapshot.total_count != 3
        || snapshot.active_count != 3
        || snapshot.completed_count != 0
    {
        return Err(format!("unexpected final snapshot counts: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.visible_titles,
        &[
            "Map realistic flows",
            "Audit launch overhead",
            "Capture settled screenshot",
        ],
        "final visible titles",
    )
}

fn assert_conduit_login_snapshot(
    snapshot: &ConduitSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("conduit login snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.route != "login"
        || snapshot.logged_in
        || !snapshot.login_visible
        || snapshot.feed_visible
        || snapshot.article_visible
    {
        return Err(format!("unexpected conduit login route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "guest"
        || snapshot.selected_slug.is_some()
        || snapshot.article_title.is_some()
        || snapshot.article_ready
    {
        return Err(format!("unexpected conduit login metadata: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_conduit_feed_snapshot(
    snapshot: &ConduitSnapshot,
    expected_favorite_count: usize,
    expected_favorited: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("conduit feed snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.route != "feed"
        || !snapshot.logged_in
        || snapshot.login_visible
        || !snapshot.feed_visible
        || snapshot.article_visible
    {
        return Err(format!("unexpected conduit feed route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "Taylor Faber"
        || snapshot.selected_slug.is_some()
        || snapshot.article_title.is_some()
        || snapshot.article_ready
    {
        return Err(format!("unexpected conduit feed metadata: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.feed_titles,
        &[
            "Waits without polling",
            CONDUIT_TITLE,
            "Actionability without jitter",
        ],
        "conduit feed titles",
    )?;
    if snapshot.composite_favorite_count != expected_favorite_count
        || snapshot.composite_favorited != expected_favorited
    {
        return Err(format!("unexpected conduit favorite state: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_conduit_article_snapshot(
    snapshot: &ConduitSnapshot,
    expected_comment_bodies: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("conduit article snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.route != "article"
        || !snapshot.logged_in
        || snapshot.login_visible
        || snapshot.feed_visible
        || !snapshot.article_visible
    {
        return Err(format!("unexpected conduit article route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "Taylor Faber"
        || snapshot.selected_slug.as_deref() != Some(CONDUIT_SLUG)
        || snapshot.article_title.as_deref() != Some(CONDUIT_TITLE)
        || !snapshot.article_ready
    {
        return Err(format!("unexpected conduit article metadata: {:?}", snapshot).into());
    }
    if snapshot.composite_favorite_count != 43 || !snapshot.composite_favorited {
        return Err(format!("unexpected conduit article favorite state: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.article_tags,
        &["waits", "networkidle", "cdp"],
        "conduit article tags",
    )?;
    expect_titles(
        &snapshot.article_comment_bodies,
        expected_comment_bodies,
        "conduit article comments",
    )?;
    Ok(())
}

async fn launch_once() -> Result<Browser, Box<dyn std::error::Error>> {
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path())
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .build()?;
    let (browser, mut handler) = Browser::launch(config).await?;
    tokio::spawn(async move { while handler.next().await.is_some() {} });
    Ok(browser)
}

async fn poll_until_true(page: &Page, expr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let ready: bool = page.evaluate(expr).await?.into_value().unwrap_or(false);
        if ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for expression: {expr}").into());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn snapshot(page: &Page) -> Result<Snapshot, Box<dyn std::error::Error>> {
    Ok(page
        .evaluate("window.__bench.snapshot()")
        .await?
        .into_value()?)
}

async fn conduit_snapshot(page: &Page) -> Result<ConduitSnapshot, Box<dyn std::error::Error>> {
    Ok(page
        .evaluate("window.__bench.snapshot()")
        .await?
        .into_value()?)
}

async fn load_initial_state(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url).await?.wait_for_navigation().await?;
    poll_until_true(page, "document.body.dataset.appReady === 'true'").await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = snapshot(page).await?;
    assert_initial_snapshot(&snap)?;
    Ok(())
}

async fn load_conduit_login(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url).await?.wait_for_navigation().await?;
    poll_until_true(page, "document.body.dataset.appReady === 'true'").await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = conduit_snapshot(page).await?;
    assert_conduit_login_snapshot(&snap)?;
    Ok(())
}

async fn add_todo(page: &Page, title: &str) -> Result<(), Box<dyn std::error::Error>> {
    let input = page.find_element(".new-todo").await?;
    input.click().await?.type_str(title).await?;
    page.find_element(".add-todo").await?.click().await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    Ok(())
}

async fn prepare_completed_view(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    add_todo(page, "Capture settled screenshot").await?;
    add_todo(page, "Trim flaky setup").await?;
    page.find_element(".todo-list li:last-child .toggle")
        .await?
        .click()
        .await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    page.find_element(".filter-completed")
        .await?
        .click()
        .await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = snapshot(page).await?;
    assert_completed_snapshot(&snap)?;
    Ok(())
}

async fn run_full_flow(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    prepare_completed_view(page).await?;
    page.find_element(".clear-completed").await?.click().await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    page.find_element(".filter-all").await?.click().await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = snapshot(page).await?;
    assert_final_snapshot(&snap)?;
    Ok(())
}

async fn conduit_login_to_feed(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.find_element(".login-submit").await?.click().await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = conduit_snapshot(page).await?;
    assert_conduit_feed_snapshot(&snap, 42, false)?;
    Ok(())
}

async fn conduit_favorite_composite(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.find_element(".favorite-button[data-slug='composite-network-idle']")
        .await?
        .click()
        .await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = conduit_snapshot(page).await?;
    assert_conduit_feed_snapshot(&snap, 43, true)?;
    Ok(())
}

async fn conduit_open_composite_article(
    page: &Page,
) -> Result<(), Box<dyn std::error::Error>> {
    page.find_element(".open-article[data-slug='composite-network-idle']")
        .await?
        .click()
        .await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = conduit_snapshot(page).await?;
    assert_conduit_article_snapshot(
        &snap,
        &[
            "The timer flush is the whole trick.",
            "Load and quiet are not the same thing.",
        ],
    )?;
    Ok(())
}

async fn conduit_post_comment(
    page: &Page,
    comment: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = page.find_element(".article-comment-input").await?;
    input.click().await?.type_str(comment).await?;
    page.find_element(".article-comment-submit")
        .await?
        .click()
        .await?;
    poll_until_true(page, "document.body.dataset.uiSettled === 'true'").await?;
    let snap = conduit_snapshot(page).await?;
    assert_conduit_article_snapshot(
        &snap,
        &[
            comment,
            "The timer flush is the whole trick.",
            "Load and quiet are not the same thing.",
        ],
    )?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = launch_once().await?;
    let page = browser.new_page(todo_url()).await?;
    let url = todo_url();
    let conduit = conduit_url();

    let mut boot_ready_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        load_initial_state(&page, &url).await?;
        boot_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_boot_ready = stats(boot_ready_samples, Some(WAIT_NOTE));
    print_stats("todomvc_boot_ready", &todomvc_boot_ready);

    let mut full_flow_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_initial_state(&page, &url).await?;
        let t = Instant::now();
        run_full_flow(&page).await?;
        full_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_full_flow = stats(full_flow_samples, Some(WAIT_NOTE));
    print_stats("todomvc_full_flow", &todomvc_full_flow);

    let mut settled_screenshot_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_initial_state(&page, &url).await?;
        prepare_completed_view(&page).await?;
        let t = Instant::now();
        page.find_element(".filter-active").await?.click().await?;
        poll_until_true(&page, "document.body.dataset.uiSettled === 'true'").await?;
        let active_snap = snapshot(&page).await?;
        assert_active_filtered_snapshot(&active_snap)?;
        let png = page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await?;
        if png.len() < 10_000 {
            return Err(format!("unexpectedly small screenshot: {} bytes", png.len()).into());
        }
        settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_settled_screenshot = stats(settled_screenshot_samples, Some(WAIT_NOTE));
    print_stats("todomvc_settled_screenshot", &todomvc_settled_screenshot);

    let mut conduit_login_ready_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        load_conduit_login(&page, &conduit).await?;
        conduit_login_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_login_ready = stats(conduit_login_ready_samples, Some(WAIT_NOTE));
    print_stats("conduit_login_ready", &conduit_login_ready);

    let mut conduit_auth_article_flow_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_conduit_login(&page, &conduit).await?;
        let t = Instant::now();
        conduit_login_to_feed(&page).await?;
        conduit_favorite_composite(&page).await?;
        conduit_open_composite_article(&page).await?;
        conduit_post_comment(&page, CONDUIT_COMMENT).await?;
        conduit_auth_article_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_auth_article_flow = stats(conduit_auth_article_flow_samples, Some(WAIT_NOTE));
    print_stats("conduit_auth_article_flow", &conduit_auth_article_flow);

    let mut conduit_article_settled_screenshot_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_conduit_login(&page, &conduit).await?;
        conduit_login_to_feed(&page).await?;
        conduit_favorite_composite(&page).await?;
        let t = Instant::now();
        conduit_open_composite_article(&page).await?;
        let png = page
            .screenshot(
                ScreenshotParams::builder()
                    .format(CaptureScreenshotFormat::Png)
                    .build(),
            )
            .await?;
        if png.len() < 15_000 {
            return Err(format!("unexpectedly small conduit screenshot: {} bytes", png.len()).into());
        }
        conduit_article_settled_screenshot_samples
            .push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_article_settled_screenshot =
        stats(conduit_article_settled_screenshot_samples, Some(WAIT_NOTE));
    print_stats(
        "conduit_article_settled_screenshot",
        &conduit_article_settled_screenshot,
    );

    println!(
        "RESULTS_JSON {}",
        json!({
            "library": "chromiumoxide",
            "scenario": "realistic",
            "metrics": {
                "todomvc_boot_ready": stats_to_json(&todomvc_boot_ready),
                "todomvc_full_flow": stats_to_json(&todomvc_full_flow),
                "todomvc_settled_screenshot": stats_to_json(&todomvc_settled_screenshot),
                "conduit_login_ready": stats_to_json(&conduit_login_ready),
                "conduit_auth_article_flow": stats_to_json(&conduit_auth_article_flow),
                "conduit_article_settled_screenshot": stats_to_json(&conduit_article_settled_screenshot),
            }
        })
    );

    Ok(())
}
