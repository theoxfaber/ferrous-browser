use anyhow::{anyhow, Result};
use headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption;
use headless_chrome::{Browser, LaunchOptions, Tab};
use realistic_server::SignalboardServer;
use serde::Deserialize;
use serde_json::json;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

const DEFAULT_ITERS: usize = 10;
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const WAIT_NOTE: &str = "manual poll @50ms for ready/settled waits";
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenverseSnapshot {
    ready: bool,
    settled: bool,
    view: String,
    query: String,
    media_type: String,
    license: String,
    skeleton_visible: bool,
    results_visible: bool,
    detail_visible: bool,
    result_count: usize,
    visible_titles: Vec<String>,
    detail_title: Option<String>,
    detail_ready: bool,
    detail_provider: Option<String>,
    detail_kind: Option<String>,
    detail_license: Option<String>,
    detail_tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RwaSnapshot {
    ready: bool,
    settled: bool,
    route: String,
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
    view: String,
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LivewireSnapshot {
    ready: bool,
    settled: bool,
    network_quiet: bool,
    view: String,
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

fn launch_signalboard_server() -> Result<(tokio::runtime::Runtime, SignalboardServer)> {
    let runtime = tokio::runtime::Runtime::new()?;
    let server = runtime.block_on(SignalboardServer::spawn());
    Ok((runtime, server))
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

fn openverse_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../realistic/fixtures/openverse/index.html")
        .canonicalize()
        .expect("canonicalize Openverse fixture");
    format!("file://{}", path.display())
}

fn rwa_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../realistic/fixtures/rwa/index.html")
        .canonicalize()
        .expect("canonicalize RWA fixture");
    format!("file://{}", path.display())
}

fn livewire_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../realistic/fixtures/livewire/index.html")
        .canonicalize()
        .expect("canonicalize Livewire fixture");
    format!("file://{}", path.display())
}

fn signalboard_run_url(base: &str, run_id: usize) -> String {
    format!("{base}?run={run_id}")
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
    let n = xs.len();
    Stats {
        median: median(xs.clone()),
        p10: p10(xs),
        n,
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

fn assert_openverse_initial_snapshot(snapshot: &OpenverseSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("openverse initial snapshot not ready: {:?}", snapshot);
    }
    if snapshot.view != "search" || !snapshot.results_visible || snapshot.detail_visible {
        anyhow::bail!("unexpected openverse initial view state: {:?}", snapshot);
    }
    if snapshot.query != "quiet cities"
        || snapshot.media_type != "all"
        || snapshot.license != "all"
        || snapshot.result_count != 4
    {
        anyhow::bail!("unexpected openverse initial filters: {:?}", snapshot);
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

fn assert_openverse_filtered_snapshot(snapshot: &OpenverseSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("openverse filtered snapshot not ready: {:?}", snapshot);
    }
    if snapshot.view != "search" || !snapshot.results_visible || snapshot.detail_visible {
        anyhow::bail!("unexpected openverse filtered view state: {:?}", snapshot);
    }
    if snapshot.media_type != "image" || snapshot.license != "cc0" || snapshot.result_count != 2 {
        anyhow::bail!("unexpected openverse filtered controls: {:?}", snapshot);
    }
    expect_titles(
        &snapshot.visible_titles,
        &["Rooftops at Noon", OPENVERSE_TARGET_TITLE],
        "openverse filtered visible titles",
    )
}

fn assert_openverse_detail_snapshot(snapshot: &OpenverseSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("openverse detail snapshot not settled: {:?}", snapshot);
    }
    if snapshot.view != "detail"
        || snapshot.results_visible
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        anyhow::bail!("unexpected openverse detail view state: {:?}", snapshot);
    }
    if snapshot.media_type != "image" || snapshot.license != "cc0" || snapshot.result_count != 2 {
        anyhow::bail!("unexpected openverse detail filters: {:?}", snapshot);
    }
    if snapshot.detail_title.as_deref() != Some(OPENVERSE_TARGET_TITLE)
        || snapshot.detail_provider.as_deref() != Some("Openverse Catalog")
        || snapshot.detail_kind.as_deref() != Some("image")
        || snapshot.detail_license.as_deref() != Some("cc0")
    {
        anyhow::bail!("unexpected openverse detail metadata: {:?}", snapshot);
    }
    expect_titles(
        &snapshot.detail_tags,
        &["masonry", "dawn", "urban"],
        "openverse detail tags",
    )
}

fn assert_rwa_login_snapshot(snapshot: &RwaSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("rwa login snapshot not ready: {:?}", snapshot);
    }
    if snapshot.route != "login"
        || snapshot.logged_in
        || !snapshot.login_visible
        || snapshot.dashboard_visible
        || snapshot.review_visible
        || snapshot.receipt_visible
    {
        anyhow::bail!("unexpected rwa login route state: {:?}", snapshot);
    }
    if snapshot.user_name != "guest" || snapshot.composer_visible || snapshot.receipt_id.is_some() {
        anyhow::bail!("unexpected rwa login metadata: {:?}", snapshot);
    }
    Ok(())
}

fn assert_rwa_dashboard_snapshot(
    snapshot: &RwaSnapshot,
    expected_composer_visible: bool,
) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("rwa dashboard snapshot not ready: {:?}", snapshot);
    }
    if snapshot.route != "dashboard"
        || !snapshot.logged_in
        || snapshot.login_visible
        || !snapshot.dashboard_visible
        || snapshot.review_visible
        || snapshot.receipt_visible
    {
        anyhow::bail!("unexpected rwa dashboard route state: {:?}", snapshot);
    }
    if snapshot.user_name != "Jordan Vale" || snapshot.composer_visible != expected_composer_visible
    {
        anyhow::bail!("unexpected rwa dashboard metadata: {:?}", snapshot);
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

fn assert_rwa_review_snapshot(snapshot: &RwaSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("rwa review snapshot not settled: {:?}", snapshot);
    }
    if snapshot.route != "review"
        || !snapshot.logged_in
        || snapshot.login_visible
        || snapshot.dashboard_visible
        || !snapshot.review_visible
        || snapshot.receipt_visible
    {
        anyhow::bail!("unexpected rwa review route state: {:?}", snapshot);
    }
    if snapshot.user_name != "Jordan Vale"
        || snapshot.draft_recipient != RWA_RECIPIENT
        || snapshot.draft_amount != RWA_AMOUNT
        || snapshot.draft_note != RWA_NOTE
        || snapshot.review_amount_cents != 12_745
    {
        anyhow::bail!("unexpected rwa review metadata: {:?}", snapshot);
    }
    Ok(())
}

fn assert_rwa_receipt_snapshot(snapshot: &RwaSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || snapshot.skeleton_visible {
        anyhow::bail!("rwa receipt snapshot not settled: {:?}", snapshot);
    }
    if snapshot.route != "receipt"
        || !snapshot.logged_in
        || snapshot.login_visible
        || snapshot.dashboard_visible
        || snapshot.review_visible
        || !snapshot.receipt_visible
    {
        anyhow::bail!("unexpected rwa receipt route state: {:?}", snapshot);
    }
    if snapshot.user_name != "Jordan Vale"
        || snapshot.receipt_id.as_deref() != Some(RWA_RECEIPT_ID)
        || snapshot.receipt_amount_label.as_deref() != Some("-$127.45")
        || snapshot.receipt_recipient.as_deref() != Some(RWA_RECIPIENT)
    {
        anyhow::bail!("unexpected rwa receipt metadata: {:?}", snapshot);
    }
    if snapshot.transaction_titles.first().map(|s| s.as_str()) != Some("Peer payment to Mina Hart")
    {
        anyhow::bail!("unexpected rwa transaction order: {:?}", snapshot);
    }
    Ok(())
}

fn assert_signalboard_ready_snapshot(snapshot: &SignalboardSnapshot) -> Result<()> {
    if !snapshot.ready {
        anyhow::bail!("signalboard ready snapshot not ready: {:?}", snapshot);
    }
    if snapshot.view != "overview"
        || snapshot.cards_visible != 3
        || snapshot.alerts_visible != 2
        || snapshot.activity_visible != 4
    {
        anyhow::bail!("unexpected signalboard ready view state: {:?}", snapshot);
    }
    if snapshot.target_card_id != SIGNALBOARD_TARGET_ID
        || snapshot.target_card_title != SIGNALBOARD_TARGET_TITLE
    {
        anyhow::bail!("unexpected signalboard ready media state: {:?}", snapshot);
    }
    Ok(())
}

fn assert_signalboard_settled_snapshot(snapshot: &SignalboardSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled {
        anyhow::bail!("signalboard settled snapshot not settled: {:?}", snapshot);
    }
    if snapshot.view != "overview"
        || snapshot.cards_visible != 3
        || snapshot.hero_images_loaded != 2
    {
        anyhow::bail!("unexpected signalboard settled view state: {:?}", snapshot);
    }
    Ok(())
}

fn assert_signalboard_quiet_snapshot(snapshot: &SignalboardSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || !snapshot.network_quiet {
        anyhow::bail!("signalboard quiet snapshot not fully quiet: {:?}", snapshot);
    }
    if snapshot.view != "overview"
        || snapshot.hero_images_loaded != 2
        || snapshot.pending_requests != 0
    {
        anyhow::bail!("unexpected signalboard quiet media state: {:?}", snapshot);
    }
    if !snapshot.insights_done || !snapshot.prefetch_done {
        anyhow::bail!(
            "unexpected signalboard quiet background state: {:?}",
            snapshot
        );
    }
    Ok(())
}

fn assert_signalboard_detail_ready_snapshot(snapshot: &SignalboardSnapshot) -> Result<()> {
    if !snapshot.ready
        || snapshot.view != "detail"
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        anyhow::bail!(
            "signalboard detail snapshot not in ready-only state: {:?}",
            snapshot
        );
    }
    if snapshot.detail_id.as_deref() != Some(SIGNALBOARD_TARGET_ID)
        || snapshot.detail_title.as_deref() != Some(SIGNALBOARD_TARGET_TITLE)
        || snapshot.detail_owner.as_deref() != Some("Runtime Operations")
        || snapshot.detail_stage_count != 3
    {
        anyhow::bail!("unexpected signalboard detail metadata: {:?}", snapshot);
    }
    Ok(())
}

fn assert_signalboard_detail_settled_snapshot(snapshot: &SignalboardSnapshot) -> Result<()> {
    if !snapshot.ready
        || !snapshot.settled
        || snapshot.view != "detail"
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        anyhow::bail!("signalboard detail snapshot not settled: {:?}", snapshot);
    }
    if snapshot.detail_id.as_deref() != Some(SIGNALBOARD_TARGET_ID)
        || snapshot.detail_title.as_deref() != Some(SIGNALBOARD_TARGET_TITLE)
        || snapshot.detail_owner.as_deref() != Some("Runtime Operations")
        || snapshot.detail_stage_count != 3
    {
        anyhow::bail!(
            "unexpected signalboard detail settled metadata: {:?}",
            snapshot
        );
    }
    if !snapshot.detail_chart_loaded {
        anyhow::bail!("signalboard detail chart not loaded: {:?}", snapshot);
    }
    Ok(())
}

fn assert_livewire_ready_snapshot(snapshot: &LivewireSnapshot) -> Result<()> {
    if !snapshot.ready {
        anyhow::bail!("livewire ready snapshot not ready: {:?}", snapshot);
    }
    if !snapshot.profile_loaded
        || snapshot.view != "overview"
        || snapshot.cards_visible != 6
        || snapshot.activity_visible != 4
    {
        anyhow::bail!("unexpected livewire ready view state: {:?}", snapshot);
    }
    if snapshot.target_card_id != LIVEWIRE_TARGET_ID
        || snapshot
            .target_card_title
            .as_deref()
            .is_none_or(|value| value.len() < 8)
    {
        anyhow::bail!("unexpected livewire ready target metadata: {:?}", snapshot);
    }
    Ok(())
}

fn assert_livewire_settled_snapshot(snapshot: &LivewireSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled {
        anyhow::bail!("livewire settled snapshot not settled: {:?}", snapshot);
    }
    if !snapshot.profile_loaded
        || snapshot.view != "overview"
        || snapshot.cards_visible != 6
        || snapshot.alerts_visible != 3
        || snapshot.hero_images_loaded != 2
    {
        anyhow::bail!("unexpected livewire settled view state: {:?}", snapshot);
    }
    Ok(())
}

fn assert_livewire_quiet_snapshot(snapshot: &LivewireSnapshot) -> Result<()> {
    if !snapshot.ready || !snapshot.settled || !snapshot.network_quiet {
        anyhow::bail!("livewire quiet snapshot not fully quiet: {:?}", snapshot);
    }
    if snapshot.view != "overview"
        || snapshot.hero_images_loaded != 2
        || snapshot.pending_requests != 0
    {
        anyhow::bail!("unexpected livewire quiet media state: {:?}", snapshot);
    }
    if !snapshot.backfill_done || !snapshot.digest_done {
        anyhow::bail!("unexpected livewire quiet background state: {:?}", snapshot);
    }
    Ok(())
}

fn assert_livewire_detail_ready_snapshot(snapshot: &LivewireSnapshot) -> Result<()> {
    if !snapshot.ready
        || snapshot.view != "detail"
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        anyhow::bail!(
            "livewire detail snapshot not in ready-only state: {:?}",
            snapshot
        );
    }
    if snapshot.detail_id != Some(LIVEWIRE_TARGET_ID)
        || snapshot.detail_title != snapshot.target_card_title
        || snapshot
            .detail_owner
            .as_deref()
            .is_none_or(|value| value.len() < 3)
        || snapshot.detail_comment_count != 3
    {
        anyhow::bail!("unexpected livewire detail metadata: {:?}", snapshot);
    }
    Ok(())
}

fn assert_livewire_detail_settled_snapshot(snapshot: &LivewireSnapshot) -> Result<()> {
    if !snapshot.ready
        || !snapshot.settled
        || snapshot.view != "detail"
        || !snapshot.detail_visible
        || !snapshot.detail_ready
    {
        anyhow::bail!("livewire detail snapshot not settled: {:?}", snapshot);
    }
    if snapshot.detail_id != Some(LIVEWIRE_TARGET_ID)
        || snapshot.detail_title != snapshot.target_card_title
        || snapshot
            .detail_owner
            .as_deref()
            .is_none_or(|value| value.len() < 3)
        || snapshot.detail_comment_count != 3
    {
        anyhow::bail!("unexpected livewire detail settled metadata: {:?}", snapshot);
    }
    if !snapshot.detail_chart_loaded {
        anyhow::bail!("livewire detail chart not loaded: {:?}", snapshot);
    }
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

fn click_selector_resilient(tab: &Tab, selector: &str) -> Result<()> {
    const MAX_ATTEMPTS: usize = 4;

    for attempt in 0..MAX_ATTEMPTS {
        match tab.wait_for_element(selector) {
            Ok(element) => match element.click() {
                Ok(_) => return Ok(()),
                Err(err) => {
                    let text = err.to_string();
                    let retryable =
                        text.contains("Node is detached from document")
                            || text.contains("ScrollingFailed")
                            || text.contains("Node is either not visible or not an HTMLElement");
                    if !retryable || attempt + 1 == MAX_ATTEMPTS {
                        return Err(err.into());
                    }
                }
            },
            Err(err) => {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(err.into());
                }
            }
        }

        sleep(POLL_INTERVAL);
    }

    anyhow::bail!("failed to click selector after retries: {selector}");
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

fn openverse_snapshot(tab: &Tab) -> Result<OpenverseSnapshot> {
    let raw = tab
        .evaluate("JSON.stringify(window.__bench.snapshot())", false)?
        .value
        .and_then(|value| value.as_str().map(|text| text.to_string()))
        .ok_or_else(|| anyhow!("missing openverse snapshot value"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn rwa_snapshot(tab: &Tab) -> Result<RwaSnapshot> {
    let raw = tab
        .evaluate("JSON.stringify(window.__bench.snapshot())", false)?
        .value
        .and_then(|value| value.as_str().map(|text| text.to_string()))
        .ok_or_else(|| anyhow!("missing rwa snapshot value"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn signalboard_snapshot(tab: &Tab) -> Result<SignalboardSnapshot> {
    let raw = tab
        .evaluate("JSON.stringify(window.__bench.snapshot())", false)?
        .value
        .and_then(|value| value.as_str().map(|text| text.to_string()))
        .ok_or_else(|| anyhow!("missing signalboard snapshot value"))?;
    Ok(serde_json::from_str(&raw)?)
}

fn livewire_snapshot(tab: &Tab) -> Result<LivewireSnapshot> {
    let raw = tab
        .evaluate("JSON.stringify(window.__bench.snapshot())", false)?
        .value
        .and_then(|value| value.as_str().map(|text| text.to_string()))
        .ok_or_else(|| anyhow!("missing livewire snapshot value"))?;
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

fn load_openverse_search(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.appReady === 'true'")?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = openverse_snapshot(tab)?;
    assert_openverse_initial_snapshot(&snap)?;
    Ok(())
}

fn load_rwa_login(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.appReady === 'true'")?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = rwa_snapshot(tab)?;
    assert_rwa_login_snapshot(&snap)?;
    Ok(())
}

fn load_signalboard_ready(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.appReady === 'true'")?;
    let snap = signalboard_snapshot(tab)?;
    assert_signalboard_ready_snapshot(&snap)?;
    Ok(())
}

fn load_signalboard_settled(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = signalboard_snapshot(tab)?;
    assert_signalboard_settled_snapshot(&snap)?;
    Ok(())
}

fn load_signalboard_quiet(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.networkQuiet === 'true'")?;
    let snap = signalboard_snapshot(tab)?;
    assert_signalboard_quiet_snapshot(&snap)?;
    Ok(())
}

fn load_livewire_ready(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.appReady === 'true'")?;
    let snap = livewire_snapshot(tab)?;
    assert_livewire_ready_snapshot(&snap)?;
    Ok(())
}

fn load_livewire_settled(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = livewire_snapshot(tab)?;
    assert_livewire_settled_snapshot(&snap)?;
    Ok(())
}

fn load_livewire_quiet(tab: &Tab, url: &str) -> Result<()> {
    tab.navigate_to(url)?.wait_until_navigated()?;
    poll_until_true(tab, "document.body.dataset.networkQuiet === 'true'")?;
    let snap = livewire_snapshot(tab)?;
    assert_livewire_quiet_snapshot(&snap)?;
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
    click_selector_resilient(tab, ".open-article[data-slug='composite-network-idle']")?;
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

fn openverse_apply_filters(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".media-image")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    tab.wait_for_element(".license-cc0")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = openverse_snapshot(tab)?;
    assert_openverse_filtered_snapshot(&snap)?;
    Ok(())
}

fn openverse_open_target_detail(tab: &Tab) -> Result<()> {
    let selector = format!(".open-detail[data-id='{OPENVERSE_TARGET_ID}']");
    click_selector_resilient(tab, &selector)?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = openverse_snapshot(tab)?;
    assert_openverse_detail_snapshot(&snap)?;
    Ok(())
}

fn rwa_login_to_dashboard(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".login-submit")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = rwa_snapshot(tab)?;
    assert_rwa_dashboard_snapshot(&snap, false)?;
    Ok(())
}

fn rwa_open_composer(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".start-payment")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = rwa_snapshot(tab)?;
    assert_rwa_dashboard_snapshot(&snap, true)?;
    Ok(())
}

fn rwa_draft_payment(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".payment-recipient")?
        .type_into(RWA_RECIPIENT)?;
    tab.wait_for_element(".payment-amount")?
        .type_into(RWA_AMOUNT)?;
    tab.wait_for_element(".payment-note")?.type_into(RWA_NOTE)?;
    Ok(())
}

fn rwa_review_payment(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".payment-review")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = rwa_snapshot(tab)?;
    assert_rwa_review_snapshot(&snap)?;
    Ok(())
}

fn rwa_submit_payment(tab: &Tab) -> Result<()> {
    tab.wait_for_element(".payment-submit")?.click()?;
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = rwa_snapshot(tab)?;
    assert_rwa_receipt_snapshot(&snap)?;
    Ok(())
}

fn signalboard_open_detail(tab: &Tab) -> Result<()> {
    let selector = format!(".open-detail[data-id='{SIGNALBOARD_TARGET_ID}']");
    click_selector_resilient(tab, &selector)?;
    poll_until_true(tab, "document.body.dataset.detailReady === 'true'")?;
    let snap = signalboard_snapshot(tab)?;
    assert_signalboard_detail_ready_snapshot(&snap)?;
    Ok(())
}

fn signalboard_wait_detail_settled(tab: &Tab) -> Result<()> {
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = signalboard_snapshot(tab)?;
    assert_signalboard_detail_settled_snapshot(&snap)?;
    Ok(())
}

fn livewire_open_detail(tab: &Tab) -> Result<()> {
    let selector = format!(".open-detail[data-id='{LIVEWIRE_TARGET_ID}']");
    click_selector_resilient(tab, &selector)?;
    poll_until_true(tab, "document.body.dataset.detailReady === 'true'")?;
    let snap = livewire_snapshot(tab)?;
    assert_livewire_detail_ready_snapshot(&snap)?;
    Ok(())
}

fn livewire_wait_detail_settled(tab: &Tab) -> Result<()> {
    poll_until_true(tab, "document.body.dataset.uiSettled === 'true'")?;
    let snap = livewire_snapshot(tab)?;
    assert_livewire_detail_settled_snapshot(&snap)?;
    Ok(())
}

fn main() -> Result<()> {
    let (_signalboard_runtime, signalboard_server) = launch_signalboard_server()?;
    let browser = launch_once()?;
    let tab = browser.new_tab()?;
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
        load_initial_state(&tab, &url)?;
        boot_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_boot_ready = stats(boot_ready_samples, Some(WAIT_NOTE));
    print_stats("todomvc_boot_ready", &todomvc_boot_ready);

    let mut full_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_initial_state(&tab, &url)?;
        let t = Instant::now();
        run_full_flow(&tab)?;
        full_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let todomvc_full_flow = stats(full_flow_samples, Some(WAIT_NOTE));
    print_stats("todomvc_full_flow", &todomvc_full_flow);

    let mut settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
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

    let mut conduit_login_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_conduit_login(&tab, &conduit)?;
        conduit_login_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let conduit_login_ready = stats(conduit_login_ready_samples, Some(WAIT_NOTE));
    print_stats("conduit_login_ready", &conduit_login_ready);

    let mut conduit_auth_article_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
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

    let mut conduit_article_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
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

    let mut openverse_search_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_openverse_search(&tab, &openverse)?;
        openverse_search_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_search_ready = stats(openverse_search_ready_samples, Some(WAIT_NOTE));
    print_stats("openverse_search_ready", &openverse_search_ready);

    let mut openverse_filter_detail_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_openverse_search(&tab, &openverse)?;
        let t = Instant::now();
        openverse_apply_filters(&tab)?;
        openverse_open_target_detail(&tab)?;
        openverse_filter_detail_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_filter_detail_flow = stats(openverse_filter_detail_flow_samples, Some(WAIT_NOTE));
    print_stats(
        "openverse_filter_detail_flow",
        &openverse_filter_detail_flow,
    );

    let mut openverse_detail_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_openverse_search(&tab, &openverse)?;
        openverse_apply_filters(&tab)?;
        let t = Instant::now();
        openverse_open_target_detail(&tab)?;
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        if png.len() < 15_000 {
            anyhow::bail!(
                "unexpectedly small openverse screenshot: {} bytes",
                png.len()
            );
        }
        openverse_detail_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let openverse_detail_settled_screenshot =
        stats(openverse_detail_settled_screenshot_samples, Some(WAIT_NOTE));
    print_stats(
        "openverse_detail_settled_screenshot",
        &openverse_detail_settled_screenshot,
    );

    let mut rwa_login_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        load_rwa_login(&tab, &rwa)?;
        rwa_login_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_login_ready = stats(rwa_login_ready_samples, Some(WAIT_NOTE));
    print_stats("rwa_login_ready", &rwa_login_ready);

    let mut rwa_payment_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_rwa_login(&tab, &rwa)?;
        let t = Instant::now();
        rwa_login_to_dashboard(&tab)?;
        rwa_open_composer(&tab)?;
        rwa_draft_payment(&tab)?;
        rwa_review_payment(&tab)?;
        rwa_submit_payment(&tab)?;
        rwa_payment_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_payment_flow = stats(rwa_payment_flow_samples, Some(WAIT_NOTE));
    print_stats("rwa_payment_flow", &rwa_payment_flow);

    let mut rwa_receipt_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        load_rwa_login(&tab, &rwa)?;
        rwa_login_to_dashboard(&tab)?;
        rwa_open_composer(&tab)?;
        rwa_draft_payment(&tab)?;
        rwa_review_payment(&tab)?;
        let t = Instant::now();
        rwa_submit_payment(&tab)?;
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        if png.len() < 15_000 {
            anyhow::bail!("unexpectedly small rwa screenshot: {} bytes", png.len());
        }
        rwa_receipt_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rwa_receipt_settled_screenshot =
        stats(rwa_receipt_settled_screenshot_samples, Some(WAIT_NOTE));
    print_stats(
        "rwa_receipt_settled_screenshot",
        &rwa_receipt_settled_screenshot,
    );

    let mut signalboard_interaction_ready_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_ready(&tab, &run_url)?;
        signalboard_interaction_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_interaction_ready =
        stats(signalboard_interaction_ready_samples, Some(WAIT_NOTE));
    print_stats(
        "signalboard_interaction_ready",
        &signalboard_interaction_ready,
    );

    let mut signalboard_visual_settled_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_settled(&tab, &run_url)?;
        signalboard_visual_settled_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_visual_settled = stats(signalboard_visual_settled_samples, Some(WAIT_NOTE));
    print_stats("signalboard_visual_settled", &signalboard_visual_settled);

    let mut signalboard_network_quiesced_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_quiet(&tab, &run_url)?;
        signalboard_network_quiesced_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_network_quiesced = stats(signalboard_network_quiesced_samples, Some(WAIT_NOTE));
    print_stats(
        "signalboard_network_quiesced",
        &signalboard_network_quiesced,
    );

    let mut signalboard_open_detail_flow_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_settled(&tab, &run_url)?;
        let t = Instant::now();
        signalboard_open_detail(&tab)?;
        signalboard_open_detail_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_open_detail_flow = stats(signalboard_open_detail_flow_samples, Some(WAIT_NOTE));
    print_stats(
        "signalboard_open_detail_flow",
        &signalboard_open_detail_flow,
    );

    let mut signalboard_detail_settled_screenshot_samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let run_url = signalboard_run_url(&signalboard, signalboard_run_id);
        signalboard_run_id += 1;
        load_signalboard_settled(&tab, &run_url)?;
        let t = Instant::now();
        signalboard_open_detail(&tab)?;
        signalboard_wait_detail_settled(&tab)?;
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        if png.len() < 15_000 {
            anyhow::bail!(
                "unexpectedly small signalboard screenshot: {} bytes",
                png.len()
            );
        }
        signalboard_detail_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let signalboard_detail_settled_screenshot = stats(
        signalboard_detail_settled_screenshot_samples,
        Some(WAIT_NOTE),
    );
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
            load_livewire_ready(&tab, &run_url)?;
            livewire_interaction_ready_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_interaction_ready_samples, Some(WAIT_NOTE));
        print_stats("livewire_interaction_ready", &livewire_stats);
        livewire_interaction_ready = Some(livewire_stats);

        let mut livewire_visual_settled_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + iters);
            let t = Instant::now();
            load_livewire_settled(&tab, &run_url)?;
            livewire_visual_settled_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_visual_settled_samples, Some(WAIT_NOTE));
        print_stats("livewire_visual_settled", &livewire_stats);
        livewire_visual_settled = Some(livewire_stats);

        let mut livewire_network_quiesced_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + (iters * 2));
            let t = Instant::now();
            load_livewire_quiet(&tab, &run_url)?;
            livewire_network_quiesced_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_network_quiesced_samples, Some(WAIT_NOTE));
        print_stats("livewire_network_quiesced", &livewire_stats);
        livewire_network_quiesced = Some(livewire_stats);

        let mut livewire_open_detail_flow_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + (iters * 3));
            load_livewire_settled(&tab, &run_url)?;
            let t = Instant::now();
            livewire_open_detail(&tab)?;
            livewire_open_detail_flow_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_open_detail_flow_samples, Some(WAIT_NOTE));
        print_stats("livewire_open_detail_flow", &livewire_stats);
        livewire_open_detail_flow = Some(livewire_stats);

        let mut livewire_detail_settled_screenshot_samples = Vec::with_capacity(iters);
        for run_id in 0..iters {
            let run_url = format!("{livewire}?run={}", run_id + (iters * 4));
            load_livewire_settled(&tab, &run_url)?;
            let t = Instant::now();
            livewire_open_detail(&tab)?;
            livewire_wait_detail_settled(&tab)?;
            let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
            if png.len() < 15_000 {
                anyhow::bail!(
                    "unexpectedly small livewire screenshot: {} bytes",
                    png.len()
                );
            }
            livewire_detail_settled_screenshot_samples.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let livewire_stats = stats(livewire_detail_settled_screenshot_samples, Some(WAIT_NOTE));
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
    metrics.insert("rwa_login_ready".into(), stats_to_json(&rwa_login_ready));
    metrics.insert("rwa_payment_flow".into(), stats_to_json(&rwa_payment_flow));
    metrics.insert(
        "rwa_receipt_settled_screenshot".into(),
        stats_to_json(&rwa_receipt_settled_screenshot),
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
            "library": "headless_chrome",
            "scenario": "realistic",
            "metrics": metrics,
        })
    );

    Ok(())
}
