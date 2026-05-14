use anyhow::{anyhow, Result};
use headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption;
use headless_chrome::{Browser, LaunchOptions, Tab};
use serde::Deserialize;
use serde_json::json;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::thread::sleep;
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

fn chrome_path() -> PathBuf {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    let home = std::env::var("HOME").expect("HOME must be set or CHROME_PATH must be provided");
    PathBuf::from(format!(
        "{home}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome"
    ))
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

fn expect_titles(actual: &[String], expected: &[&str], label: &str) -> Result<()> {
    let expected_vec: Vec<String> = expected.iter().map(|value| value.to_string()).collect();
    if actual != expected_vec {
        anyhow::bail!("{label}: expected {:?}, got {:?}", expected_vec, actual);
    }
    Ok(())
}

fn assert_initial_snapshot(snapshot: &Snapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("initial snapshot not ready: {:?}", snapshot);
    }
    if snapshot.filter != "all"
        || snapshot.total_count != 3
        || snapshot.active_count != 2
        || snapshot.completed_count != 1
    {
        anyhow::bail!("unexpected initial counts: {:?}", snapshot);
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

fn assert_completed_snapshot(snapshot: &Snapshot) -> Result<()> {
    if snapshot.filter != "completed" || snapshot.completed_count != 2 {
        anyhow::bail!("unexpected completed snapshot: {:?}", snapshot);
    }
    expect_titles(
        &snapshot.visible_titles,
        &["Ship stable waits", "Trim flaky setup"],
        "completed visible titles",
    )
}

fn assert_active_filtered_snapshot(snapshot: &Snapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("active-filter snapshot not settled: {:?}", snapshot);
    }
    if snapshot.filter != "active"
        || snapshot.total_count != 5
        || snapshot.active_count != 3
        || snapshot.completed_count != 2
    {
        anyhow::bail!("unexpected active-filter counts: {:?}", snapshot);
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

fn assert_final_snapshot(snapshot: &Snapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("final snapshot not settled: {:?}", snapshot);
    }
    if snapshot.filter != "all"
        || snapshot.total_count != 3
        || snapshot.active_count != 3
        || snapshot.completed_count != 0
    {
        anyhow::bail!("unexpected final snapshot counts: {:?}", snapshot);
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

fn assert_conduit_login_snapshot(snapshot: &ConduitSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("conduit login snapshot not ready: {:?}", snapshot);
    }
    if snapshot.route != "login"
        || snapshot.logged_in
        || !snapshot.login_visible
        || snapshot.feed_visible
        || snapshot.article_visible
    {
        anyhow::bail!("unexpected conduit login route state: {:?}", snapshot);
    }
    if snapshot.user_name != "guest"
        || snapshot.selected_slug.is_some()
        || snapshot.article_title.is_some()
        || snapshot.article_ready
    {
        anyhow::bail!("unexpected conduit login metadata: {:?}", snapshot);
    }
    Ok(())
}

fn assert_conduit_feed_snapshot(
    snapshot: &ConduitSnapshot,
    expected_favorite_count: usize,
    expected_favorited: bool,
) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("conduit feed snapshot not ready: {:?}", snapshot);
    }
    if snapshot.route != "feed"
        || !snapshot.logged_in
        || snapshot.login_visible
        || !snapshot.feed_visible
        || snapshot.article_visible
    {
        anyhow::bail!("unexpected conduit feed route state: {:?}", snapshot);
    }
    if snapshot.user_name != "Taylor Faber"
        || snapshot.selected_slug.is_some()
        || snapshot.article_title.is_some()
        || snapshot.article_ready
    {
        anyhow::bail!("unexpected conduit feed metadata: {:?}", snapshot);
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
        anyhow::bail!("unexpected conduit favorite state: {:?}", snapshot);
    }
    Ok(())
}

fn assert_conduit_article_snapshot(
    snapshot: &ConduitSnapshot,
    expected_comment_bodies: &[&str],
) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("conduit article snapshot not settled: {:?}", snapshot);
    }
    if snapshot.route != "article"
        || !snapshot.logged_in
        || snapshot.login_visible
        || snapshot.feed_visible
        || !snapshot.article_visible
    {
        anyhow::bail!("unexpected conduit article route state: {:?}", snapshot);
    }
    if snapshot.user_name != "Taylor Faber"
        || snapshot.selected_slug.as_deref() != Some(CONDUIT_SLUG)
        || snapshot.article_title.as_deref() != Some(CONDUIT_TITLE)
        || !snapshot.article_ready
    {
        anyhow::bail!("unexpected conduit article metadata: {:?}", snapshot);
    }
    if snapshot.composite_favorite_count != 43 || !snapshot.composite_favorited {
        anyhow::bail!("unexpected conduit article favorite state: {:?}", snapshot);
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

fn launch_once() -> Result<Browser> {
    let extra_args: Vec<&OsStr> = vec![
        OsStr::new("--disable-gpu"),
        OsStr::new("--disable-dev-shm-usage"),
    ];
    let options = LaunchOptions {
        headless: true,
        sandbox: false,
        path: Some(chrome_path()),
        args: extra_args,
        ..Default::default()
    };
    Ok(Browser::new(options)?)
}

fn poll_until_true(tab: &Tab, expr: &str) -> Result<()> {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let ready = tab
            .evaluate(expr, false)?
            .value
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if ready {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for expression: {expr}");
        }
        sleep(POLL_INTERVAL);
    }
}

fn snapshot(tab: &Tab) -> Result<Snapshot> {
    let raw = tab
        .evaluate("JSON.stringify(window.__bench.snapshot())", false)?
        .value
        .and_then(|value| value.as_str().map(|text| text.to_string()))
        .ok_or_else(|| anyhow!("missing snapshot value"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn conduit_snapshot(tab: &Tab) -> Result<ConduitSnapshot> {
    let raw = tab
        .evaluate("JSON.stringify(window.__bench.snapshot())", false)?
        .value
        .and_then(|value| value.as_str().map(|text| text.to_string()))
        .ok_or_else(|| anyhow!("missing conduit snapshot value"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn load_initial_state(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.appReady === 'true'")?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = snapshot(tab)?;
    assert_initial_snapshot(&snap)?;
    Ok(())
}

fn load_conduit_login(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.appReady === 'true'")?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = conduit_snapshot(tab)?;
    assert_conduit_login_snapshot(&snap)?;
    Ok(())
}

fn add_todo(tab: &Tab, title: &str) -> Result<()> {
    tab.wait_for_element(".new-todo")?.type_into(title)?;
    tab.wait_for_element(".add-todo")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    Ok(())
}

fn prepare_completed_view(tab: &Tab) -> Result<()> {
    add_todo(tab, "Capture settled screenshot")?;
    add_todo(tab, "Trim flaky setup")?;
    tab.wait_for_element(".todo-list li:last-child .toggle")?
        .click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    tab.wait_for_element(".filter-completed")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = snapshot(tab)?;
    assert_completed_snapshot(&snap)?;
    Ok(())
}

fn run_full_flow(tab: &Tab) -> Result<()> {
    prepare_completed_view(tab)?;
    tab.wait_for_element(".clear-completed")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    tab.wait_for_element(".filter-all")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = snapshot(tab)?;
    assert_final_snapshot(&snap)?;
    Ok(())
}

fn conduit_login_to_feed(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".login-submit")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = conduit_snapshot(tab)?;
    assert_conduit_feed_snapshot(&snap, 42, false)?;
    Ok(())
}

fn conduit_favorite_composite(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".favorite-button[data-slug='composite-network-idle']")?
        .click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = conduit_snapshot(tab)?;
    assert_conduit_feed_snapshot(&snap, 43, true)?;
    Ok(())
}

fn conduit_open_composite_article(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".open-article[data-slug='composite-network-idle']")?
        .click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = conduit_snapshot(tab)?;
    assert_conduit_article_snapshot(
        &snap,
        &[
            "The timer flush is the whole trick.",
            "Load and quiet are not the same thing.",
        ],
    )?;
    Ok(())
}

fn conduit_post_comment(tab: &Tab, comment: &str) -> Result<()> {
    tab.wait_for_element(".article-comment-input")?
        .type_into(comment)?;
    tab.wait_for_element(".article-comment-submit")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = conduit_snapshot(tab)?;
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

fn main() -> Result<()> {
    let browser = launch_once()?;
    let tab = browser.new_tab()?;
    let url = todo_url();
    let conduit = conduit_url();

    let mut boot_ready_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        load_initial_state(&tab, &url)?;
        boot_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_boot_ready = stats(boot_ready_samples, Some(WAIT_NOTE));
    print_stats("todomvc_boot_ready", &todomvc_boot_ready);

    let mut full_flow_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_initial_state(&tab, &url)?;
        let t = Instant::now();
        run_full_flow(&tab)?;
        full_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_full_flow = stats(full_flow_samples, Some(WAIT_NOTE));
    print_stats("todomvc_full_flow", &todomvc_full_flow);

    let mut settled_screenshot_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_initial_state(&tab, &url)?;
        prepare_completed_view(&tab)?;
        let t = Instant::now();
        tab.wait_for_element(".filter-active")?.click()?;
        poll_until_true(&tab, "document.body.dataset.uiSettled === 'true'")?;
        let active_snap = snapshot(&tab)?;
        assert_active_filtered_snapshot(&active_snap)?;
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        if png.len() < 10_000 {
            anyhow::bail!("unexpectedly small screenshot: {} bytes", png.len());
        }
        settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_settled_screenshot = stats(settled_screenshot_samples, Some(WAIT_NOTE));
    print_stats("todomvc_settled_screenshot", &todomvc_settled_screenshot);

    let mut conduit_login_ready_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        load_conduit_login(&tab, &conduit)?;
        conduit_login_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_login_ready = stats(conduit_login_ready_samples, Some(WAIT_NOTE));
    print_stats("conduit_login_ready", &conduit_login_ready);

    let mut conduit_auth_article_flow_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_conduit_login(&tab, &conduit)?;
        let t = Instant::now();
        conduit_login_to_feed(&tab)?;
        conduit_favorite_composite(&tab)?;
        conduit_open_composite_article(&tab)?;
        conduit_post_comment(&tab, CONDUIT_COMMENT)?;
        conduit_auth_article_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_auth_article_flow = stats(conduit_auth_article_flow_samples, Some(WAIT_NOTE));
    print_stats("conduit_auth_article_flow", &conduit_auth_article_flow);

    let mut conduit_article_settled_screenshot_samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        load_conduit_login(&tab, &conduit)?;
        conduit_login_to_feed(&tab)?;
        conduit_favorite_composite(&tab)?;
        let t = Instant::now();
        conduit_open_composite_article(&tab)?;
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        if png.len() < 15_000 {
            anyhow::bail!("unexpectedly small conduit screenshot: {} bytes", png.len());
        }
        conduit_article_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
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
            "library": "headless_chrome",
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
