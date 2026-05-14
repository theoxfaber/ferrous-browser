use ferrous_browser::{Browser, BrowserConfig, Page, ScreenshotOptions, WaitUntil};
use realistic_server::SignalboardServer;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DEFAULT_ITERS: usize = 10;
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const SETTLED_EXPR: &str = "document.body.dataset.uiSettled === 'true'";
const READY_AND_SETTLED_EXPR: &str =
    "document.body.dataset.appReady === 'true' && document.body.dataset.uiSettled === 'true'";
const SNAPSHOT_EXPR: &str = "window.__bench.snapshot()";
const CONDUIT_SLUG: &str = "composite-network-idle";
const CONDUIT_TITLE: &str = "Composite NetworkIdle";
const CONDUIT_COMMENT: &str = "Benchmark the real flow.";
const OPENVERSE_TARGET_ID: &str = "quiet-morning-stacks";
const OPENVERSE_TARGET_TITLE: &str = "Quiet Morning Stacks";
const RWA_RECIPIENT: &str = "Mina Hart";
const RWA_AMOUNT: &str = "127.45";
const RWA_NOTE: &str = "Benchmark seeded payment.";
const RWA_RECEIPT_ID: &str = "TX-3020";
const SIGNALBOARD_TARGET_ID: &str = "latency-lab";
const SIGNALBOARD_TARGET_TITLE: &str = "Latency Lab";
const LIVEWIRE_TARGET_ID: usize = 11;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum TodoFilter {
    All,
    Active,
    Completed,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ConduitRoute {
    Login,
    Feed,
    Article,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum OpenverseView {
    Search,
    Detail,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum OpenverseMediaType {
    All,
    Image,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum OpenverseLicense {
    All,
    Cc0,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum OpenverseDetailKind {
    Image,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum RwaRoute {
    Login,
    Dashboard,
    Review,
    Receipt,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum SignalboardView {
    Overview,
    Detail,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum LivewireView {
    Overview,
    Detail,
}

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
    filter: TodoFilter,
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
    route: ConduitRoute,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenverseSnapshot {
    ready: bool,
    settled: bool,
    view: OpenverseView,
    query: String,
    media_type: OpenverseMediaType,
    license: OpenverseLicense,
    skeleton_visible: bool,
    results_visible: bool,
    detail_visible: bool,
    result_count: usize,
    visible_titles: Vec<String>,
    detail_title: Option<String>,
    detail_ready: bool,
    detail_provider: Option<String>,
    detail_kind: Option<OpenverseDetailKind>,
    detail_license: Option<OpenverseLicense>,
    detail_tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RwaSnapshot {
    ready: bool,
    settled: bool,
    route: RwaRoute,
    logged_in: bool,
    skeleton_visible: bool,
    login_visible: bool,
    dashboard_visible: bool,
    review_visible: bool,
    receipt_visible: bool,
    composer_visible: bool,
    user_name: String,
    transaction_titles: Vec<String>,
    draft_recipient: String,
    draft_amount: String,
    draft_note: String,
    review_amount_cents: usize,
    receipt_id: Option<String>,
    receipt_amount_label: Option<String>,
    receipt_recipient: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignalboardSnapshot {
    ready: bool,
    settled: bool,
    network_quiet: bool,
    view: SignalboardView,
    cards_visible: usize,
    alerts_visible: usize,
    activity_visible: usize,
    hero_images_loaded: usize,
    insights_done: bool,
    prefetch_done: bool,
    pending_requests: usize,
    target_card_id: String,
    target_card_title: String,
    detail_visible: bool,
    detail_id: Option<String>,
    detail_title: Option<String>,
    detail_owner: Option<String>,
    detail_stage_count: usize,
    detail_ready: bool,
    detail_chart_loaded: bool,
    detail_audit_done: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LivewireSnapshot {
    ready: bool,
    settled: bool,
    network_quiet: bool,
    view: LivewireView,
    profile_loaded: bool,
    cards_visible: usize,
    alerts_visible: usize,
    activity_visible: usize,
    hero_images_loaded: usize,
    backfill_done: bool,
    digest_done: bool,
    pending_requests: usize,
    target_card_id: usize,
    target_card_title: Option<String>,
    detail_visible: bool,
    detail_id: Option<usize>,
    detail_title: Option<String>,
    detail_owner: Option<String>,
    detail_comment_count: usize,
    detail_ready: bool,
    detail_chart_loaded: bool,
}

/// Linear-interpolation quantile (numpy's default `method="linear"`).
fn quantile_sorted(xs: &[f64], q: f64) -> f64 {
    let n = xs.len();
    if n == 0 {
        return f64::NAN;
    }
    if n == 1 {
        return xs[0];
    }
    let pos = (n - 1) as f64 * q;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let frac = pos - lo as f64;
    xs[lo] + frac * (xs[hi] - xs[lo])
}

fn stats(mut xs: Vec<f64>) -> Stats {
    xs.sort_by(f64::total_cmp);
    let n = xs.len();
    Stats {
        median: quantile_sorted(&xs, 0.50),
        p10: quantile_sorted(&xs, 0.10),
        n,
        note: None,
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

fn iterations() -> usize {
    std::env::var("ITERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_ITERS)
}

fn live_internet_enabled() -> bool {
    matches!(
        std::env::var("LIVE_INTERNET"),
        Ok(value) if matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
    )
}

fn bench_chrome_path() -> String {
    if let Ok(path) = std::env::var("CHROME_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let home = std::env::var("HOME").expect("HOME must be set or CHROME_PATH must be provided");
    format!("{home}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome")
}

fn bench_browser_config() -> BrowserConfig {
    BrowserConfig {
        chrome_path: Some(bench_chrome_path()),
        ..Default::default()
    }
}

fn todo_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench/realistic/fixtures/todomvc/index.html")
        .canonicalize()
        .expect("canonicalize TodoMVC fixture");
    format!("file://{}", path.display())
}

fn conduit_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench/realistic/fixtures/conduit/index.html")
        .canonicalize()
        .expect("canonicalize Conduit fixture");
    format!("file://{}", path.display())
}

fn openverse_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench/realistic/fixtures/openverse/index.html")
        .canonicalize()
        .expect("canonicalize Openverse fixture");
    format!("file://{}", path.display())
}

fn rwa_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench/realistic/fixtures/rwa/index.html")
        .canonicalize()
        .expect("canonicalize RWA fixture");
    format!("file://{}", path.display())
}

fn livewire_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bench/realistic/fixtures/livewire/index.html")
        .canonicalize()
        .expect("canonicalize Livewire fixture");
    format!("file://{}", path.display())
}

fn signalboard_run_url(base: &str, run_id: usize) -> String {
    format!("{base}?run={run_id}")
}

async fn wait_settled(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.wait_for_function(SETTLED_EXPR, WAIT_TIMEOUT).await?;
    Ok(())
}

async fn wait_network_quiet(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.wait_for_function(
        "document.body.dataset.networkQuiet === 'true'",
        WAIT_TIMEOUT,
    )
    .await?;
    Ok(())
}

async fn settled_snapshot<T: DeserializeOwned>(
    page: &Page,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(page
        .wait_for_function_value(SETTLED_EXPR, SNAPSHOT_EXPR, WAIT_TIMEOUT)
        .await?)
}

async fn ready_and_settled_snapshot<T: DeserializeOwned>(
    page: &Page,
) -> Result<T, Box<dyn std::error::Error>> {
    Ok(page
        .wait_for_function_value(READY_AND_SETTLED_EXPR, SNAPSHOT_EXPR, WAIT_TIMEOUT)
        .await?)
}

async fn capture_png(
    page: &Page,
    options: ScreenshotOptions,
    min_size: usize,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let png = page.screenshot_with_options(options).await?;
    if png.len() < min_size {
        return Err(format!("unexpectedly small {label}: {} bytes", png.len()).into());
    }
    Ok(())
}

fn expect_titles(
    actual: &[String],
    expected: &[&str],
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !actual
        .iter()
        .map(String::as_str)
        .eq(expected.iter().copied())
    {
        return Err(format!("{label}: expected {:?}, got {:?}", expected, actual).into());
    }
    Ok(())
}

fn assert_initial_snapshot(snapshot: &Snapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("initial snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.filter != TodoFilter::All
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
    if snapshot.filter != TodoFilter::Completed || snapshot.completed_count != 2 {
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
    if snapshot.filter != TodoFilter::Active
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
    if snapshot.filter != TodoFilter::All
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
    if snapshot.route != ConduitRoute::Login
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
    if snapshot.route != ConduitRoute::Feed
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
    if snapshot.route != ConduitRoute::Article
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

fn assert_openverse_initial_snapshot(
    snapshot: &OpenverseSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("openverse initial snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.view != OpenverseView::Search
        || !snapshot.results_visible
        || snapshot.detail_visible
    {
        return Err(format!("unexpected openverse initial view state: {:?}", snapshot).into());
    }
    if snapshot.query != "quiet cities"
        || snapshot.media_type != OpenverseMediaType::All
        || snapshot.license != OpenverseLicense::All
        || snapshot.result_count != 4
    {
        return Err(format!("unexpected openverse initial filters: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.visible_titles,
        &[
            "Rooftops at Noon",
            "Streetcar Ambience",
            OPENVERSE_TARGET_TITLE,
            "Marble Atrium",
        ],
        "openverse initial visible titles",
    )
}

fn assert_openverse_filtered_snapshot(
    snapshot: &OpenverseSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("openverse filtered snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.view != OpenverseView::Search
        || !snapshot.results_visible
        || snapshot.detail_visible
    {
        return Err(format!("unexpected openverse filtered view state: {:?}", snapshot).into());
    }
    if snapshot.media_type != OpenverseMediaType::Image
        || snapshot.license != OpenverseLicense::Cc0
        || snapshot.result_count != 2
    {
        return Err(format!("unexpected openverse filtered controls: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.visible_titles,
        &["Rooftops at Noon", OPENVERSE_TARGET_TITLE],
        "openverse filtered visible titles",
    )
}

fn assert_openverse_detail_snapshot(
    snapshot: &OpenverseSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("openverse detail snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.view != OpenverseView::Detail
        || snapshot.results_visible
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        return Err(format!("unexpected openverse detail view state: {:?}", snapshot).into());
    }
    if snapshot.media_type != OpenverseMediaType::Image
        || snapshot.license != OpenverseLicense::Cc0
        || snapshot.result_count != 2
    {
        return Err(format!("unexpected openverse detail filters: {:?}", snapshot).into());
    }
    if snapshot.detail_title.as_deref() != Some(OPENVERSE_TARGET_TITLE)
        || snapshot.detail_provider.as_deref() != Some("Openverse Catalog")
        || snapshot.detail_kind != Some(OpenverseDetailKind::Image)
        || snapshot.detail_license != Some(OpenverseLicense::Cc0)
    {
        return Err(format!("unexpected openverse detail metadata: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.detail_tags,
        &["masonry", "dawn", "urban"],
        "openverse detail tags",
    )
}

fn assert_rwa_login_snapshot(snapshot: &RwaSnapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("rwa login snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.route != RwaRoute::Login
        || snapshot.logged_in
        || !snapshot.login_visible
        || snapshot.dashboard_visible
        || snapshot.review_visible
        || snapshot.receipt_visible
    {
        return Err(format!("unexpected rwa login route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "guest" || snapshot.composer_visible || snapshot.receipt_id.is_some() {
        return Err(format!("unexpected rwa login metadata: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_rwa_dashboard_snapshot(
    snapshot: &RwaSnapshot,
    expected_composer_visible: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("rwa dashboard snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.route != RwaRoute::Dashboard
        || !snapshot.logged_in
        || snapshot.login_visible
        || !snapshot.dashboard_visible
        || snapshot.review_visible
        || snapshot.receipt_visible
    {
        return Err(format!("unexpected rwa dashboard route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "Jordan Vale" || snapshot.composer_visible != expected_composer_visible
    {
        return Err(format!("unexpected rwa dashboard metadata: {:?}", snapshot).into());
    }
    expect_titles(
        &snapshot.transaction_titles,
        &[
            "Payroll adjustment",
            "Operations rent",
            "Travel reimbursement",
        ],
        "rwa dashboard transactions",
    )
}

fn assert_rwa_review_snapshot(snapshot: &RwaSnapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("rwa review snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.route != RwaRoute::Review
        || !snapshot.logged_in
        || snapshot.login_visible
        || snapshot.dashboard_visible
        || !snapshot.review_visible
        || snapshot.receipt_visible
    {
        return Err(format!("unexpected rwa review route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "Jordan Vale"
        || snapshot.draft_recipient != RWA_RECIPIENT
        || snapshot.draft_amount != RWA_AMOUNT
        || snapshot.draft_note != RWA_NOTE
        || snapshot.review_amount_cents != 12_745
    {
        return Err(format!("unexpected rwa review metadata: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_rwa_receipt_snapshot(snapshot: &RwaSnapshot) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        return Err(format!("rwa receipt snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.route != RwaRoute::Receipt
        || !snapshot.logged_in
        || snapshot.login_visible
        || snapshot.dashboard_visible
        || snapshot.review_visible
        || !snapshot.receipt_visible
    {
        return Err(format!("unexpected rwa receipt route state: {:?}", snapshot).into());
    }
    if snapshot.user_name != "Jordan Vale"
        || snapshot.receipt_id.as_deref() != Some(RWA_RECEIPT_ID)
        || snapshot.receipt_amount_label.as_deref() != Some("-$127.45")
        || snapshot.receipt_recipient.as_deref() != Some(RWA_RECIPIENT)
    {
        return Err(format!("unexpected rwa receipt metadata: {:?}", snapshot).into());
    }
    if snapshot.transaction_titles.first().map(|s| s.as_str()) != Some("Peer payment to Mina Hart")
    {
        return Err(format!("unexpected rwa transaction order: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_signalboard_ready_snapshot(
    snapshot: &SignalboardSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || snapshot.settled || snapshot.network_quiet {
        return Err(format!("signalboard ready snapshot not ready-only: {:?}", snapshot).into());
    }
    if snapshot.view != SignalboardView::Overview
        || snapshot.cards_visible != 3
        || snapshot.alerts_visible != 2
        || snapshot.activity_visible != 4
    {
        return Err(format!("unexpected signalboard ready view state: {:?}", snapshot).into());
    }
    if snapshot.hero_images_loaded >= 2
        || snapshot.target_card_id != SIGNALBOARD_TARGET_ID
        || snapshot.target_card_title != SIGNALBOARD_TARGET_TITLE
    {
        return Err(format!("unexpected signalboard ready media state: {:?}", snapshot).into());
    }
    if snapshot.pending_requests <= 0 || snapshot.insights_done || snapshot.prefetch_done {
        return Err(format!(
            "unexpected signalboard ready background state: {:?}",
            snapshot
        )
        .into());
    }
    Ok(())
}

fn assert_signalboard_settled_snapshot(
    snapshot: &SignalboardSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || snapshot.network_quiet {
        return Err(format!(
            "signalboard settled snapshot not in visual-settled state: {:?}",
            snapshot
        )
        .into());
    }
    if snapshot.view != SignalboardView::Overview
        || snapshot.cards_visible != 3
        || snapshot.hero_images_loaded != 2
    {
        return Err(format!("unexpected signalboard settled view state: {:?}", snapshot).into());
    }
    if snapshot.pending_requests <= 0 || snapshot.insights_done || snapshot.prefetch_done {
        return Err(format!(
            "unexpected signalboard settled background state: {:?}",
            snapshot
        )
        .into());
    }
    Ok(())
}

fn assert_signalboard_quiet_snapshot(
    snapshot: &SignalboardSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || !snapshot.network_quiet {
        return Err(format!("signalboard quiet snapshot not fully quiet: {:?}", snapshot).into());
    }
    if snapshot.view != SignalboardView::Overview
        || snapshot.hero_images_loaded != 2
        || snapshot.pending_requests != 0
    {
        return Err(format!("unexpected signalboard quiet media state: {:?}", snapshot).into());
    }
    if !snapshot.insights_done || !snapshot.prefetch_done {
        return Err(format!(
            "unexpected signalboard quiet background state: {:?}",
            snapshot
        )
        .into());
    }
    Ok(())
}

fn assert_signalboard_detail_ready_snapshot(
    snapshot: &SignalboardSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready
        || snapshot.settled
        || snapshot.view != SignalboardView::Detail
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        return Err(format!(
            "signalboard detail snapshot not in ready-only state: {:?}",
            snapshot
        )
        .into());
    }
    if snapshot.detail_id.as_deref() != Some(SIGNALBOARD_TARGET_ID)
        || snapshot.detail_title.as_deref() != Some(SIGNALBOARD_TARGET_TITLE)
        || snapshot.detail_owner.as_deref() != Some("Runtime Operations")
        || snapshot.detail_stage_count != 3
    {
        return Err(format!("unexpected signalboard detail metadata: {:?}", snapshot).into());
    }
    if snapshot.detail_chart_loaded || snapshot.detail_audit_done || snapshot.pending_requests <= 0
    {
        return Err(format!(
            "unexpected signalboard detail background state: {:?}",
            snapshot
        )
        .into());
    }
    Ok(())
}

fn assert_signalboard_detail_settled_snapshot(
    snapshot: &SignalboardSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready
        || !snapshot.settled
        || snapshot.view != SignalboardView::Detail
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        return Err(format!("signalboard detail snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.detail_id.as_deref() != Some(SIGNALBOARD_TARGET_ID)
        || snapshot.detail_title.as_deref() != Some(SIGNALBOARD_TARGET_TITLE)
        || snapshot.detail_owner.as_deref() != Some("Runtime Operations")
        || snapshot.detail_stage_count != 3
    {
        return Err(format!(
            "unexpected signalboard detail settled metadata: {:?}",
            snapshot
        )
        .into());
    }
    if !snapshot.detail_chart_loaded {
        return Err(format!("signalboard detail chart not loaded: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_livewire_ready_snapshot(
    snapshot: &LivewireSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready {
        return Err(format!("livewire ready snapshot not ready: {:?}", snapshot).into());
    }
    if !snapshot.profile_loaded
        || snapshot.view != LivewireView::Overview
        || snapshot.cards_visible != 6
        || snapshot.activity_visible != 4
    {
        return Err(format!("unexpected livewire ready view state: {:?}", snapshot).into());
    }
    if snapshot.target_card_id != LIVEWIRE_TARGET_ID
        || snapshot
            .target_card_title
            .as_deref()
            .is_none_or(|value| value.len() < 8)
    {
        return Err(format!("unexpected livewire ready target metadata: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_livewire_settled_snapshot(
    snapshot: &LivewireSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled {
        return Err(format!("livewire settled snapshot not settled: {:?}", snapshot).into());
    }
    if !snapshot.profile_loaded
        || snapshot.view != LivewireView::Overview
        || snapshot.cards_visible != 6
        || snapshot.alerts_visible != 3
        || snapshot.hero_images_loaded != 2
    {
        return Err(format!("unexpected livewire settled view state: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_livewire_quiet_snapshot(
    snapshot: &LivewireSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || !snapshot.settled || !snapshot.network_quiet {
        return Err(format!("livewire quiet snapshot not fully quiet: {:?}", snapshot).into());
    }
    if snapshot.view != LivewireView::Overview
        || snapshot.hero_images_loaded != 2
        || snapshot.pending_requests != 0
    {
        return Err(format!("unexpected livewire quiet media state: {:?}", snapshot).into());
    }
    if !snapshot.backfill_done || !snapshot.digest_done {
        return Err(format!("unexpected livewire quiet background state: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_livewire_detail_ready_snapshot(
    snapshot: &LivewireSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready || snapshot.view != LivewireView::Detail || !snapshot.detail_visible || !snapshot.detail_ready {
        return Err(format!("livewire detail snapshot not ready: {:?}", snapshot).into());
    }
    if snapshot.detail_id != Some(LIVEWIRE_TARGET_ID)
        || snapshot.detail_title != snapshot.target_card_title
        || snapshot
            .detail_owner
            .as_deref()
            .is_none_or(|value| value.len() < 3)
        || snapshot.detail_comment_count != 3
    {
        return Err(format!("unexpected livewire detail metadata: {:?}", snapshot).into());
    }
    Ok(())
}

fn assert_livewire_detail_settled_snapshot(
    snapshot: &LivewireSnapshot,
) -> Result<(), Box<dyn std::error::Error>> {
    if !snapshot.ready
        || !snapshot.settled
        || snapshot.view != LivewireView::Detail
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        return Err(format!("livewire detail snapshot not settled: {:?}", snapshot).into());
    }
    if snapshot.detail_id != Some(LIVEWIRE_TARGET_ID)
        || snapshot.detail_title != snapshot.target_card_title
        || snapshot
            .detail_owner
            .as_deref()
            .is_none_or(|value| value.len() < 3)
        || snapshot.detail_comment_count != 3
    {
        return Err(
            format!("unexpected livewire detail settled metadata: {:?}", snapshot).into(),
        );
    }
    if !snapshot.detail_chart_loaded {
        return Err(format!("livewire detail chart not loaded: {:?}", snapshot).into());
    }
    Ok(())
}

async fn load_initial_state(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: Snapshot = ready_and_settled_snapshot(page).await?;
    assert_initial_snapshot(&snap)?;
    Ok(())
}

async fn load_conduit_login(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: ConduitSnapshot = ready_and_settled_snapshot(page).await?;
    assert_conduit_login_snapshot(&snap)?;
    Ok(())
}

async fn load_openverse_search(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: OpenverseSnapshot = ready_and_settled_snapshot(page).await?;
    assert_openverse_initial_snapshot(&snap)?;
    Ok(())
}

async fn load_rwa_login(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: RwaSnapshot = ready_and_settled_snapshot(page).await?;
    assert_rwa_login_snapshot(&snap)?;
    Ok(())
}

async fn load_signalboard_ready(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: SignalboardSnapshot = page
        .wait_for_function_value(
            "document.body.dataset.appReady === 'true'",
            SNAPSHOT_EXPR,
            WAIT_TIMEOUT,
        )
        .await?;
    assert_signalboard_ready_snapshot(&snap)?;
    Ok(())
}

async fn load_signalboard_settled(
    page: &Page,
    url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: SignalboardSnapshot = ready_and_settled_snapshot(page).await?;
    assert_signalboard_settled_snapshot(&snap)?;
    Ok(())
}

async fn load_signalboard_quiet(page: &Page, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    wait_network_quiet(page).await?;
    let snap: SignalboardSnapshot = page.evaluate(SNAPSHOT_EXPR).await?;
    assert_signalboard_quiet_snapshot(&snap)?;
    Ok(())
}

async fn add_todo(page: &Page, title: &str) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".new-todo").type_text(title).await?;
    page.locator(".add-todo").click_auto().await?;
    wait_settled(page).await?;
    Ok(())
}

async fn prepare_completed_view(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    add_todo(page, "Capture settled screenshot").await?;
    add_todo(page, "Trim flaky setup").await?;
    page.locator(".todo-list li:last-child .toggle")
        .click_auto()
        .await?;
    wait_settled(page).await?;
    page.locator(".filter-completed").click_auto().await?;
    wait_settled(page).await?;
    let snap: Snapshot = settled_snapshot(page).await?;
    assert_completed_snapshot(&snap)?;
    Ok(())
}

async fn run_full_flow(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    prepare_completed_view(page).await?;
    page.locator(".clear-completed").click_auto().await?;
    wait_settled(page).await?;
    page.locator(".filter-all").click_auto().await?;
    let snap: Snapshot = settled_snapshot(page).await?;
    assert_final_snapshot(&snap)?;
    Ok(())
}

async fn conduit_login_to_feed(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".login-submit").click_auto().await?;
    let snap: ConduitSnapshot = settled_snapshot(page).await?;
    assert_conduit_feed_snapshot(&snap, 42, false)?;
    Ok(())
}

async fn conduit_favorite_composite(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".favorite-button[data-slug='composite-network-idle']")
        .click_auto()
        .await?;
    let snap: ConduitSnapshot = settled_snapshot(page).await?;
    assert_conduit_feed_snapshot(&snap, 43, true)?;
    Ok(())
}

async fn conduit_open_composite_article(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".open-article[data-slug='composite-network-idle']")
        .click_auto()
        .await?;
    let snap: ConduitSnapshot = settled_snapshot(page).await?;
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
    page.locator(".article-comment-input")
        .type_text(comment)
        .await?;
    page.locator(".article-comment-submit").click_auto().await?;
    let snap: ConduitSnapshot = settled_snapshot(page).await?;
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

async fn openverse_apply_filters(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".media-image").click_auto().await?;
    wait_settled(page).await?;
    page.locator(".license-cc0").click_auto().await?;
    let snap: OpenverseSnapshot = settled_snapshot(page).await?;
    assert_openverse_filtered_snapshot(&snap)?;
    Ok(())
}

async fn openverse_open_target_detail(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    let selector = format!(".open-detail[data-id='{OPENVERSE_TARGET_ID}']");
    page.locator(&selector).click_auto().await?;
    let snap: OpenverseSnapshot = settled_snapshot(page).await?;
    assert_openverse_detail_snapshot(&snap)?;
    Ok(())
}

async fn rwa_login_to_dashboard(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".login-submit").click_auto().await?;
    let snap: RwaSnapshot = settled_snapshot(page).await?;
    assert_rwa_dashboard_snapshot(&snap, false)?;
    Ok(())
}

async fn rwa_open_composer(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".start-payment").click_auto().await?;
    let snap: RwaSnapshot = settled_snapshot(page).await?;
    assert_rwa_dashboard_snapshot(&snap, true)?;
    Ok(())
}

async fn rwa_draft_payment(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".payment-recipient")
        .type_text(RWA_RECIPIENT)
        .await?;
    page.locator(".payment-amount")
        .type_text(RWA_AMOUNT)
        .await?;
    page.locator(".payment-note").type_text(RWA_NOTE).await?;
    Ok(())
}

async fn rwa_review_payment(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".payment-review").click_auto().await?;
    let snap: RwaSnapshot = settled_snapshot(page).await?;
    assert_rwa_review_snapshot(&snap)?;
    Ok(())
}

async fn rwa_submit_payment(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    page.locator(".payment-submit").click_auto().await?;
    let snap: RwaSnapshot = settled_snapshot(page).await?;
    assert_rwa_receipt_snapshot(&snap)?;
    Ok(())
}

async fn signalboard_open_detail(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    let selector = format!(".open-detail[data-id='{SIGNALBOARD_TARGET_ID}']");
    page.locator(&selector).click_auto().await?;
    let snap: SignalboardSnapshot = page
        .wait_for_function_value(
            "document.body.dataset.detailReady === 'true'",
            SNAPSHOT_EXPR,
            WAIT_TIMEOUT,
        )
        .await?;
    assert_signalboard_detail_ready_snapshot(&snap)?;
    Ok(())
}

async fn signalboard_wait_detail_settled(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    let snap: SignalboardSnapshot = settled_snapshot(page).await?;
    assert_signalboard_detail_settled_snapshot(&snap)?;
    Ok(())
}

async fn load_livewire_ready(
    page: &Page,
    url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: LivewireSnapshot = page
        .wait_for_function_value("document.body.dataset.appReady === 'true'", SNAPSHOT_EXPR, WAIT_TIMEOUT)
        .await?;
    assert_livewire_ready_snapshot(&snap)?;
    Ok(())
}

async fn load_livewire_settled(
    page: &Page,
    url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    let snap: LivewireSnapshot = settled_snapshot(page).await?;
    assert_livewire_settled_snapshot(&snap)?;
    Ok(())
}

async fn load_livewire_quiet(
    page: &Page,
    url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    page.goto(url, WaitUntil::Load).await?;
    page.wait_for_function(
        "document.body.dataset.networkQuiet === 'true'",
        WAIT_TIMEOUT,
    )
    .await?;
    let snap: LivewireSnapshot = page.evaluate(SNAPSHOT_EXPR).await?;
    assert_livewire_quiet_snapshot(&snap)?;
    Ok(())
}

async fn livewire_open_detail(page: &Page) -> Result<(), Box<dyn std::error::Error>> {
    let selector = format!(".open-detail[data-id='{LIVEWIRE_TARGET_ID}']");
    page.locator(&selector).click_auto().await?;
    let snap: LivewireSnapshot = page
        .wait_for_function_value(
            "document.body.dataset.detailReady === 'true'",
            SNAPSHOT_EXPR,
            WAIT_TIMEOUT,
        )
        .await?;
    assert_livewire_detail_ready_snapshot(&snap)?;
    Ok(())
}

async fn livewire_wait_detail_settled(
    page: &Page,
) -> Result<(), Box<dyn std::error::Error>> {
    let snap: LivewireSnapshot = settled_snapshot(page).await?;
    assert_livewire_detail_settled_snapshot(&snap)?;
    Ok(())
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let signalboard_server = SignalboardServer::spawn().await;
    let browser = Browser::launch_chrome(Some(bench_browser_config())).await?;
    let page = browser.new_page().await?;
    let url = todo_url();
    let conduit = conduit_url();
    let openverse = openverse_url();
    let rwa = rwa_url();
    let livewire = livewire_url();
    let live_internet = live_internet_enabled();
    let signalboard = signalboard_server.url();
    let mut signalboard_run_id = 0_usize;
    let iters = iterations();

    let mut boot_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_initial_state(&page, &url).await?;
        boot_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_boot_ready = stats(boot_ready_samples);
    print_stats("todomvc_boot_ready", &todomvc_boot_ready);

    let mut full_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_initial_state(&page, &url).await?;
        let t = Instant::now();
        run_full_flow(&page).await?;
        full_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_full_flow = stats(full_flow_samples);
    print_stats("todomvc_full_flow", &todomvc_full_flow);

    let mut settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_initial_state(&page, &url).await?;
        prepare_completed_view(&page).await?;
        let t = Instant::now();
        page.locator(".filter-active").click_auto().await?;
        let active_snap: Snapshot = settled_snapshot(&page).await?;
        assert_active_filtered_snapshot(&active_snap)?;
        capture_png(&page, ScreenshotOptions::fast_png(), 10_000, "screenshot").await?;
        settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_settled_screenshot = stats(settled_screenshot_samples);
    print_stats("todomvc_settled_screenshot", &todomvc_settled_screenshot);

    let mut settled_screenshot_conservative_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_initial_state(&page, &url).await?;
        prepare_completed_view(&page).await?;
        let t = Instant::now();
        page.locator(".filter-active").click_auto().await?;
        let active_snap: Snapshot = settled_snapshot(&page).await?;
        assert_active_filtered_snapshot(&active_snap)?;
        capture_png(
            &page,
            ScreenshotOptions::default(),
            10_000,
            "conservative screenshot",
        )
        .await?;
        settled_screenshot_conservative_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_settled_screenshot_conservative_png =
        stats(settled_screenshot_conservative_samples);
    print_stats(
        "todomvc_settled_screenshot_conservative_png",
        &todomvc_settled_screenshot_conservative_png,
    );

    let mut conduit_login_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_conduit_login(&page, &conduit).await?;
        conduit_login_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_login_ready = stats(conduit_login_ready_samples);
    print_stats("conduit_login_ready", &conduit_login_ready);

    let mut conduit_auth_article_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_conduit_login(&page, &conduit).await?;
        let t = Instant::now();
        conduit_login_to_feed(&page).await?;
        conduit_favorite_composite(&page).await?;
        conduit_open_composite_article(&page).await?;
        conduit_post_comment(&page, CONDUIT_COMMENT).await?;
        conduit_auth_article_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_auth_article_flow = stats(conduit_auth_article_flow_samples);
    print_stats("conduit_auth_article_flow", &conduit_auth_article_flow);

    let mut conduit_article_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_conduit_login(&page, &conduit).await?;
        conduit_login_to_feed(&page).await?;
        conduit_favorite_composite(&page).await?;
        let t = Instant::now();
        conduit_open_composite_article(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::fast_png(),
            15_000,
            "conduit screenshot",
        )
        .await?;
        conduit_article_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_article_settled_screenshot = stats(conduit_article_settled_screenshot_samples);
    print_stats(
        "conduit_article_settled_screenshot",
        &conduit_article_settled_screenshot,
    );

    let mut conduit_article_settled_screenshot_conservative_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_conduit_login(&page, &conduit).await?;
        conduit_login_to_feed(&page).await?;
        conduit_favorite_composite(&page).await?;
        let t = Instant::now();
        conduit_open_composite_article(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::default(),
            15_000,
            "conservative conduit screenshot",
        )
        .await?;
        conduit_article_settled_screenshot_conservative_samples
            .push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_article_settled_screenshot_conservative_png =
        stats(conduit_article_settled_screenshot_conservative_samples);
    print_stats(
        "conduit_article_settled_screenshot_conservative_png",
        &conduit_article_settled_screenshot_conservative_png,
    );

    let mut openverse_search_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_openverse_search(&page, &openverse).await?;
        openverse_search_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_search_ready = stats(openverse_search_ready_samples);
    print_stats("openverse_search_ready", &openverse_search_ready);

    let mut openverse_filter_detail_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_openverse_search(&page, &openverse).await?;
        let t = Instant::now();
        openverse_apply_filters(&page).await?;
        openverse_open_target_detail(&page).await?;
        openverse_filter_detail_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_filter_detail_flow = stats(openverse_filter_detail_flow_samples);
    print_stats(
        "openverse_filter_detail_flow",
        &openverse_filter_detail_flow,
    );

    let mut openverse_detail_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_openverse_search(&page, &openverse).await?;
        openverse_apply_filters(&page).await?;
        let t = Instant::now();
        openverse_open_target_detail(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::fast_png(),
            15_000,
            "openverse screenshot",
        )
        .await?;
        openverse_detail_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_detail_settled_screenshot = stats(openverse_detail_settled_screenshot_samples);
    print_stats(
        "openverse_detail_settled_screenshot",
        &openverse_detail_settled_screenshot,
    );

    let mut openverse_detail_settled_screenshot_conservative_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_openverse_search(&page, &openverse).await?;
        openverse_apply_filters(&page).await?;
        let t = Instant::now();
        openverse_open_target_detail(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::default(),
            15_000,
            "conservative openverse screenshot",
        )
        .await?;
        openverse_detail_settled_screenshot_conservative_samples
            .push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_detail_settled_screenshot_conservative_png =
        stats(openverse_detail_settled_screenshot_conservative_samples);
    print_stats(
        "openverse_detail_settled_screenshot_conservative_png",
        &openverse_detail_settled_screenshot_conservative_png,
    );

    let mut rwa_login_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_rwa_login(&page, &rwa).await?;
        rwa_login_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_login_ready = stats(rwa_login_ready_samples);
    print_stats("rwa_login_ready", &rwa_login_ready);

    let mut rwa_payment_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_rwa_login(&page, &rwa).await?;
        let t = Instant::now();
        rwa_login_to_dashboard(&page).await?;
        rwa_open_composer(&page).await?;
        rwa_draft_payment(&page).await?;
        rwa_review_payment(&page).await?;
        rwa_submit_payment(&page).await?;
        rwa_payment_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_payment_flow = stats(rwa_payment_flow_samples);
    print_stats("rwa_payment_flow", &rwa_payment_flow);

    let mut rwa_receipt_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_rwa_login(&page, &rwa).await?;
        rwa_login_to_dashboard(&page).await?;
        rwa_open_composer(&page).await?;
        rwa_draft_payment(&page).await?;
        rwa_review_payment(&page).await?;
        let t = Instant::now();
        rwa_submit_payment(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::fast_png(),
            15_000,
            "rwa screenshot",
        )
        .await?;
        rwa_receipt_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_receipt_settled_screenshot = stats(rwa_receipt_settled_screenshot_samples);
    print_stats(
        "rwa_receipt_settled_screenshot",
        &rwa_receipt_settled_screenshot,
    );

    let mut rwa_receipt_settled_screenshot_conservative_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_rwa_login(&page, &rwa).await?;
        rwa_login_to_dashboard(&page).await?;
        rwa_open_composer(&page).await?;
        rwa_draft_payment(&page).await?;
        rwa_review_payment(&page).await?;
        let t = Instant::now();
        rwa_submit_payment(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::default(),
            15_000,
            "conservative rwa screenshot",
        )
        .await?;
        rwa_receipt_settled_screenshot_conservative_samples
            .push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_receipt_settled_screenshot_conservative_png =
        stats(rwa_receipt_settled_screenshot_conservative_samples);
    print_stats(
        "rwa_receipt_settled_screenshot_conservative_png",
        &rwa_receipt_settled_screenshot_conservative_png,
    );

    let mut signalboard_interaction_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_ready(&page, &run_url).await?;
        signalboard_interaction_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_interaction_ready = stats(signalboard_interaction_ready_samples);
    print_stats(
        "signalboard_interaction_ready",
        &signalboard_interaction_ready,
    );

    let mut signalboard_visual_settled_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_settled(&page, &run_url).await?;
        signalboard_visual_settled_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_visual_settled = stats(signalboard_visual_settled_samples);
    print_stats("signalboard_visual_settled", &signalboard_visual_settled);

    let mut signalboard_network_quiesced_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_quiet(&page, &run_url).await?;
        signalboard_network_quiesced_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_network_quiesced = stats(signalboard_network_quiesced_samples);
    print_stats(
        "signalboard_network_quiesced",
        &signalboard_network_quiesced,
    );

    let mut signalboard_open_detail_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_settled(&page, &run_url).await?;
        let t = Instant::now();
        signalboard_open_detail(&page).await?;
        signalboard_open_detail_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_open_detail_flow = stats(signalboard_open_detail_flow_samples);
    print_stats(
        "signalboard_open_detail_flow",
        &signalboard_open_detail_flow,
    );

    let mut signalboard_detail_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_settled(&page, &run_url).await?;
        let t = Instant::now();
        signalboard_open_detail(&page).await?;
        signalboard_wait_detail_settled(&page).await?;
        capture_png(
            &page,
            ScreenshotOptions::fast_png(),
            15_000,
            "signalboard screenshot",
        )
        .await?;
        signalboard_detail_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_detail_settled_screenshot =
        stats(signalboard_detail_settled_screenshot_samples);
    print_stats(
        "signalboard_detail_settled_screenshot",
        &signalboard_detail_settled_screenshot,
    );

    let mut livewire_interaction_ready = None;
    let mut livewire_visual_settled = None;
    let mut livewire_network_quiesced = None;
    let mut livewire_open_detail_flow = None;
    let mut livewire_detail_settled_screenshot = None;

    if live_internet {
        let mut livewire_interaction_ready_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={run_id}");
            let t = Instant::now();
            load_livewire_ready(&page, &run_url).await?;
            livewire_interaction_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_interaction_ready_samples);
        print_stats("livewire_interaction_ready", &livewire_stats);
        livewire_interaction_ready = Some(livewire_stats);

        let mut livewire_visual_settled_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + iters);
            let t = Instant::now();
            load_livewire_settled(&page, &run_url).await?;
            livewire_visual_settled_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_visual_settled_samples);
        print_stats("livewire_visual_settled", &livewire_stats);
        livewire_visual_settled = Some(livewire_stats);

        let mut livewire_network_quiesced_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + (iters * 2));
            let t = Instant::now();
            load_livewire_quiet(&page, &run_url).await?;
            livewire_network_quiesced_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_network_quiesced_samples);
        print_stats("livewire_network_quiesced", &livewire_stats);
        livewire_network_quiesced = Some(livewire_stats);

        let mut livewire_open_detail_flow_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + (iters * 3));
            load_livewire_settled(&page, &run_url).await?;
            let t = Instant::now();
            livewire_open_detail(&page).await?;
            livewire_open_detail_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_open_detail_flow_samples);
        print_stats("livewire_open_detail_flow", &livewire_stats);
        livewire_open_detail_flow = Some(livewire_stats);

        let mut livewire_detail_settled_screenshot_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + (iters * 4));
            load_livewire_settled(&page, &run_url).await?;
            let t = Instant::now();
            livewire_open_detail(&page).await?;
            livewire_wait_detail_settled(&page).await?;
            capture_png(
                &page,
                ScreenshotOptions::fast_png(),
                15_000,
                "livewire screenshot",
            )
            .await?;
            livewire_detail_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_detail_settled_screenshot_samples);
        print_stats("livewire_detail_settled_screenshot", &livewire_stats);
        livewire_detail_settled_screenshot = Some(livewire_stats);
    }

    let mut metrics = serde_json::Map::new();
    metrics.insert(
        "todomvc_boot_ready".into(),
        stats_to_json(&todomvc_boot_ready),
    );
    metrics.insert("todomvc_full_flow".into(), stats_to_json(&todomvc_full_flow));
    metrics.insert(
        "todomvc_settled_screenshot".into(),
        stats_to_json(&todomvc_settled_screenshot),
    );
    metrics.insert(
        "todomvc_settled_screenshot_conservative_png".into(),
        stats_to_json(&todomvc_settled_screenshot_conservative_png),
    );
    metrics.insert("conduit_login_ready".into(), stats_to_json(&conduit_login_ready));
    metrics.insert(
        "conduit_auth_article_flow".into(),
        stats_to_json(&conduit_auth_article_flow),
    );
    metrics.insert(
        "conduit_article_settled_screenshot".into(),
        stats_to_json(&conduit_article_settled_screenshot),
    );
    metrics.insert(
        "conduit_article_settled_screenshot_conservative_png".into(),
        stats_to_json(&conduit_article_settled_screenshot_conservative_png),
    );
    metrics.insert(
        "openverse_search_ready".into(),
        stats_to_json(&openverse_search_ready),
    );
    metrics.insert(
        "openverse_filter_detail_flow".into(),
        stats_to_json(&openverse_filter_detail_flow),
    );
    metrics.insert(
        "openverse_detail_settled_screenshot".into(),
        stats_to_json(&openverse_detail_settled_screenshot),
    );
    metrics.insert(
        "openverse_detail_settled_screenshot_conservative_png".into(),
        stats_to_json(&openverse_detail_settled_screenshot_conservative_png),
    );
    metrics.insert("rwa_login_ready".into(), stats_to_json(&rwa_login_ready));
    metrics.insert("rwa_payment_flow".into(), stats_to_json(&rwa_payment_flow));
    metrics.insert(
        "rwa_receipt_settled_screenshot".into(),
        stats_to_json(&rwa_receipt_settled_screenshot),
    );
    metrics.insert(
        "rwa_receipt_settled_screenshot_conservative_png".into(),
        stats_to_json(&rwa_receipt_settled_screenshot_conservative_png),
    );
    metrics.insert(
        "signalboard_interaction_ready".into(),
        stats_to_json(&signalboard_interaction_ready),
    );
    metrics.insert(
        "signalboard_visual_settled".into(),
        stats_to_json(&signalboard_visual_settled),
    );
    metrics.insert(
        "signalboard_network_quiesced".into(),
        stats_to_json(&signalboard_network_quiesced),
    );
    metrics.insert(
        "signalboard_open_detail_flow".into(),
        stats_to_json(&signalboard_open_detail_flow),
    );
    metrics.insert(
        "signalboard_detail_settled_screenshot".into(),
        stats_to_json(&signalboard_detail_settled_screenshot),
    );
    if let Some(stats) = &livewire_interaction_ready {
        metrics.insert("livewire_interaction_ready".into(), stats_to_json(stats));
    }
    if let Some(stats) = &livewire_visual_settled {
        metrics.insert("livewire_visual_settled".into(), stats_to_json(stats));
    }
    if let Some(stats) = &livewire_network_quiesced {
        metrics.insert("livewire_network_quiesced".into(), stats_to_json(stats));
    }
    if let Some(stats) = &livewire_open_detail_flow {
        metrics.insert("livewire_open_detail_flow".into(), stats_to_json(stats));
    }
    if let Some(stats) = &livewire_detail_settled_screenshot {
        metrics.insert("livewire_detail_settled_screenshot".into(), stats_to_json(stats));
    }

    println!(
        "RESULTS_JSON {}",
        json!({
            "library": "ferrous-browser",
            "scenario": "realistic",
            "metrics": metrics,
        })
    );

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run().await
}
