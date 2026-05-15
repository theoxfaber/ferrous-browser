const path = require('path');
const { performance } = require('perf_hooks');
const { pathToFileURL } = require('url');

const ROOT = path.resolve(__dirname, '..', '..');
const ITERS = Number(process.env.ITERS || '10');
const LIVE_INTERNET = /^(1|true|yes)$/i.test(process.env.LIVE_INTERNET || '');
const CHROME_PATH = process.env.CHROME_PATH
  || `${process.env.HOME}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome`;

function median(xs) {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

function p10(xs) {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length * 0.1)];
}

function stats(xs, note = null) {
  return { median: median(xs), p10: p10(xs), n: xs.length, note };
}

function printStats(name, s) {
  const base = `${name.padEnd(28)} median=${s.median.toFixed(2)}ms p10=${s.p10.toFixed(2)}ms (n=${s.n})`;
  console.log(s.note ? `${base} [${s.note}]` : base);
}

async function timed(fn) {
  const t0 = performance.now();
  await fn();
  return performance.now() - t0;
}

function todoMvcUrl() {
  return pathToFileURL(path.join(__dirname, 'fixtures', 'todomvc', 'index.html')).href;
}

function conduitUrl() {
  return pathToFileURL(path.join(__dirname, 'fixtures', 'conduit', 'index.html')).href;
}

function openverseUrl() {
  return pathToFileURL(path.join(__dirname, 'fixtures', 'openverse', 'index.html')).href;
}

function rwaUrl() {
  return pathToFileURL(path.join(__dirname, 'fixtures', 'rwa', 'index.html')).href;
}

function livewireUrl() {
  return pathToFileURL(path.join(__dirname, 'fixtures', 'livewire', 'index.html')).href;
}

const CONDUIT_ARTICLE_SLUG = 'composite-network-idle';
const CONDUIT_ARTICLE_TITLE = 'Composite NetworkIdle';
const CONDUIT_FLOW_COMMENT = 'Benchmark the real flow.';
const OPENVERSE_TARGET_ID = 'quiet-morning-stacks';
const OPENVERSE_TARGET_TITLE = 'Quiet Morning Stacks';
const RWA_RECIPIENT = 'Mina Hart';
const RWA_AMOUNT = '127.45';
const RWA_NOTE = 'Benchmark seeded payment.';
const RWA_RECEIPT_ID = 'TX-3020';
const SIGNALBOARD_TARGET_ID = 'latency-lab';
const SIGNALBOARD_TARGET_TITLE = 'Latency Lab';
const LIVEWIRE_TARGET_ID = 11;

function expectArrayEqual(actual, expected, label) {
  if (actual.length !== expected.length) {
    throw new Error(`${label}: expected ${expected.length} items, got ${actual.length}`);
  }
  for (let i = 0; i < expected.length; i++) {
    if (actual[i] !== expected[i]) {
      throw new Error(`${label}: mismatch at ${i}: expected "${expected[i]}", got "${actual[i]}"`);
    }
  }
}

function assertInitialSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`initial snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.filter !== 'all' || snapshot.totalCount !== 3 || snapshot.activeCount !== 2 || snapshot.completedCount !== 1) {
    throw new Error(`unexpected initial counts: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.visibleTitles, [
    'Map realistic flows',
    'Ship stable waits',
    'Audit launch overhead',
  ], 'initial visible titles');
}

function assertCompletedSnapshot(snapshot) {
  if (snapshot.filter !== 'completed' || snapshot.completedCount !== 2) {
    throw new Error(`unexpected completed snapshot: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.visibleTitles, [
    'Ship stable waits',
    'Trim flaky setup',
  ], 'completed visible titles');
}

function assertFinalSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`final snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.filter !== 'all' || snapshot.totalCount !== 3 || snapshot.activeCount !== 3 || snapshot.completedCount !== 0) {
    throw new Error(`unexpected final counts: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.visibleTitles, [
    'Map realistic flows',
    'Audit launch overhead',
    'Capture settled screenshot',
  ], 'final visible titles');
}

function assertActiveFilteredSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`active-filter snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.filter !== 'active' || snapshot.totalCount !== 5 || snapshot.activeCount !== 3 || snapshot.completedCount !== 2) {
    throw new Error(`unexpected active-filter counts: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.visibleTitles, [
    'Map realistic flows',
    'Audit launch overhead',
    'Capture settled screenshot',
  ], 'active-filter visible titles');
}

function assertConduitLoginSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`conduit login snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'login' || snapshot.loggedIn || !snapshot.loginVisible || snapshot.feedVisible || snapshot.articleVisible) {
    throw new Error(`unexpected conduit login route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'guest' || snapshot.selectedSlug !== null || snapshot.articleTitle !== null || snapshot.articleReady) {
    throw new Error(`unexpected conduit login metadata: ${JSON.stringify(snapshot)}`);
  }
}

function assertConduitFeedSnapshot(snapshot, expectedFavoriteCount, expectedFavorited) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`conduit feed snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'feed' || !snapshot.loggedIn || snapshot.loginVisible || !snapshot.feedVisible || snapshot.articleVisible) {
    throw new Error(`unexpected conduit feed route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'Taylor Faber' || snapshot.selectedSlug !== null || snapshot.articleTitle !== null || snapshot.articleReady) {
    throw new Error(`unexpected conduit feed metadata: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.feedTitles, [
    'Waits without polling',
    CONDUIT_ARTICLE_TITLE,
    'Actionability without jitter',
  ], 'conduit feed titles');
  if (snapshot.compositeFavoriteCount !== expectedFavoriteCount || snapshot.compositeFavorited !== expectedFavorited) {
    throw new Error(`unexpected conduit favorite state: ${JSON.stringify(snapshot)}`);
  }
}

function assertConduitArticleSnapshot(snapshot, expectedCommentBodies) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`conduit article snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'article' || !snapshot.loggedIn || snapshot.loginVisible || snapshot.feedVisible || !snapshot.articleVisible) {
    throw new Error(`unexpected conduit article route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'Taylor Faber' || snapshot.selectedSlug !== CONDUIT_ARTICLE_SLUG || snapshot.articleTitle !== CONDUIT_ARTICLE_TITLE || !snapshot.articleReady) {
    throw new Error(`unexpected conduit article metadata: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.compositeFavoriteCount !== 43 || !snapshot.compositeFavorited) {
    throw new Error(`unexpected conduit article favorite state: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.articleTags, ['waits', 'networkidle', 'cdp'], 'conduit article tags');
  expectArrayEqual(snapshot.articleCommentBodies, expectedCommentBodies, 'conduit article comments');
}

function assertOpenverseInitialSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`openverse initial snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'search' || !snapshot.resultsVisible || snapshot.detailVisible) {
    throw new Error(`unexpected openverse initial view state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.query !== 'quiet cities' || snapshot.mediaType !== 'all' || snapshot.license !== 'all' || snapshot.resultCount !== 4) {
    throw new Error(`unexpected openverse initial filters: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.visibleTitles, [
    'Rooftops at Noon',
    'Streetcar Ambience',
    OPENVERSE_TARGET_TITLE,
    'Marble Atrium',
  ], 'openverse initial visible titles');
}

function assertOpenverseFilteredSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`openverse filtered snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'search' || !snapshot.resultsVisible || snapshot.detailVisible) {
    throw new Error(`unexpected openverse filtered view state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.mediaType !== 'image' || snapshot.license !== 'cc0' || snapshot.resultCount !== 2) {
    throw new Error(`unexpected openverse filtered controls: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.visibleTitles, [
    'Rooftops at Noon',
    OPENVERSE_TARGET_TITLE,
  ], 'openverse filtered visible titles');
}

function assertOpenverseDetailSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`openverse detail snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'detail' || snapshot.resultsVisible || !snapshot.detailVisible || !snapshot.detailReady) {
    throw new Error(`unexpected openverse detail view state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.mediaType !== 'image' || snapshot.license !== 'cc0' || snapshot.resultCount !== 2) {
    throw new Error(`unexpected openverse detail filters: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.detailTitle !== OPENVERSE_TARGET_TITLE || snapshot.detailProvider !== 'Openverse Catalog' || snapshot.detailKind !== 'image' || snapshot.detailLicense !== 'cc0') {
    throw new Error(`unexpected openverse detail metadata: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.detailTags, ['masonry', 'dawn', 'urban'], 'openverse detail tags');
}

function assertRwaLoginSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`rwa login snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'login' || snapshot.loggedIn || !snapshot.loginVisible || snapshot.dashboardVisible || snapshot.reviewVisible || snapshot.receiptVisible) {
    throw new Error(`unexpected rwa login route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'guest' || snapshot.composerVisible || snapshot.receiptId !== null) {
    throw new Error(`unexpected rwa login metadata: ${JSON.stringify(snapshot)}`);
  }
}

function assertRwaDashboardSnapshot(snapshot, expectedComposerVisible) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`rwa dashboard snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'dashboard' || !snapshot.loggedIn || snapshot.loginVisible || !snapshot.dashboardVisible || snapshot.reviewVisible || snapshot.receiptVisible) {
    throw new Error(`unexpected rwa dashboard route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'Jordan Vale' || snapshot.composerVisible !== expectedComposerVisible) {
    throw new Error(`unexpected rwa dashboard metadata: ${JSON.stringify(snapshot)}`);
  }
  expectArrayEqual(snapshot.transactionTitles, [
    'Payroll adjustment',
    'Operations rent',
    'Travel reimbursement',
  ], 'rwa dashboard transactions');
}

function assertRwaReviewSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`rwa review snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'review' || !snapshot.loggedIn || snapshot.loginVisible || snapshot.dashboardVisible || !snapshot.reviewVisible || snapshot.receiptVisible) {
    throw new Error(`unexpected rwa review route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'Jordan Vale' || snapshot.draftRecipient !== RWA_RECIPIENT || snapshot.draftAmount !== RWA_AMOUNT || snapshot.draftNote !== RWA_NOTE || snapshot.reviewAmountCents !== 12745) {
    throw new Error(`unexpected rwa review metadata: ${JSON.stringify(snapshot)}`);
  }
}

function assertRwaReceiptSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.skeletonVisible) {
    throw new Error(`rwa receipt snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.route !== 'receipt' || !snapshot.loggedIn || snapshot.loginVisible || snapshot.dashboardVisible || snapshot.reviewVisible || !snapshot.receiptVisible) {
    throw new Error(`unexpected rwa receipt route state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.userName !== 'Jordan Vale' || snapshot.receiptId !== RWA_RECEIPT_ID || snapshot.receiptAmountLabel !== '-$127.45' || snapshot.receiptRecipient !== RWA_RECIPIENT) {
    throw new Error(`unexpected rwa receipt metadata: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.transactionTitles[0] !== `Peer payment to ${RWA_RECIPIENT}`) {
    throw new Error(`unexpected rwa transaction order: ${JSON.stringify(snapshot)}`);
  }
}

function assertSignalboardReadySnapshot(snapshot) {
  if (!snapshot.ready || snapshot.settled || snapshot.networkQuiet) {
    throw new Error(`signalboard ready snapshot not in ready-only state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'overview' || snapshot.cardsVisible !== 3 || snapshot.alertsVisible !== 2 || snapshot.activityVisible !== 4) {
    throw new Error(`unexpected signalboard ready view state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.heroImagesLoaded >= 2 || snapshot.targetCardId !== SIGNALBOARD_TARGET_ID || snapshot.targetCardTitle !== SIGNALBOARD_TARGET_TITLE) {
    throw new Error(`unexpected signalboard ready media state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.pendingRequests <= 0 || snapshot.insightsDone || snapshot.prefetchDone) {
    throw new Error(`unexpected signalboard ready background state: ${JSON.stringify(snapshot)}`);
  }
}

function assertSignalboardSettledSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.networkQuiet) {
    throw new Error(`signalboard settled snapshot not in visual-settled state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'overview' || snapshot.cardsVisible !== 3 || snapshot.heroImagesLoaded !== 2) {
    throw new Error(`unexpected signalboard settled view state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.pendingRequests <= 0 || snapshot.insightsDone || snapshot.prefetchDone) {
    throw new Error(`unexpected signalboard settled background state: ${JSON.stringify(snapshot)}`);
  }
}

function assertSignalboardQuietSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || !snapshot.networkQuiet) {
    throw new Error(`signalboard quiet snapshot not fully quiet: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'overview' || snapshot.heroImagesLoaded !== 2 || snapshot.pendingRequests !== 0) {
    throw new Error(`unexpected signalboard quiet media state: ${JSON.stringify(snapshot)}`);
  }
  if (!snapshot.insightsDone || !snapshot.prefetchDone) {
    throw new Error(`unexpected signalboard quiet background state: ${JSON.stringify(snapshot)}`);
  }
}

function assertSignalboardDetailReadySnapshot(snapshot) {
  if (!snapshot.ready || snapshot.settled || snapshot.view !== 'detail' || !snapshot.detailVisible || !snapshot.detailReady) {
    throw new Error(`signalboard detail snapshot not in ready-only state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.detailId !== SIGNALBOARD_TARGET_ID || snapshot.detailTitle !== SIGNALBOARD_TARGET_TITLE || snapshot.detailOwner !== 'Runtime Operations' || snapshot.detailStageCount !== 3) {
    throw new Error(`unexpected signalboard detail metadata: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.detailChartLoaded || snapshot.detailAuditDone || snapshot.pendingRequests <= 0) {
    throw new Error(`unexpected signalboard detail background state: ${JSON.stringify(snapshot)}`);
  }
}

function assertSignalboardDetailSettledSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.view !== 'detail' || !snapshot.detailVisible || !snapshot.detailReady) {
    throw new Error(`signalboard detail snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.detailId !== SIGNALBOARD_TARGET_ID || snapshot.detailTitle !== SIGNALBOARD_TARGET_TITLE || snapshot.detailOwner !== 'Runtime Operations' || snapshot.detailStageCount !== 3) {
    throw new Error(`unexpected signalboard detail settled metadata: ${JSON.stringify(snapshot)}`);
  }
  if (!snapshot.detailChartLoaded) {
    throw new Error(`signalboard detail chart not loaded: ${JSON.stringify(snapshot)}`);
  }
}

function assertLivewireReadySnapshot(snapshot) {
  if (!snapshot.ready) {
    throw new Error(`livewire ready snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (!snapshot.profileLoaded || snapshot.view !== 'overview' || snapshot.cardsVisible !== 6 || snapshot.activityVisible !== 4) {
    throw new Error(`unexpected livewire ready view state: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.targetCardId !== LIVEWIRE_TARGET_ID || typeof snapshot.targetCardTitle !== 'string' || snapshot.targetCardTitle.length < 8) {
    throw new Error(`unexpected livewire ready target metadata: ${JSON.stringify(snapshot)}`);
  }
}

function assertLivewireSettledSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled) {
    throw new Error(`livewire settled snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (!snapshot.profileLoaded || snapshot.view !== 'overview' || snapshot.cardsVisible !== 6 || snapshot.alertsVisible !== 3 || snapshot.heroImagesLoaded !== 2) {
    throw new Error(`unexpected livewire settled view state: ${JSON.stringify(snapshot)}`);
  }
}

function assertLivewireQuietSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || !snapshot.networkQuiet) {
    throw new Error(`livewire quiet snapshot not fully quiet: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.view !== 'overview' || snapshot.heroImagesLoaded !== 2 || snapshot.pendingRequests !== 0) {
    throw new Error(`unexpected livewire quiet media state: ${JSON.stringify(snapshot)}`);
  }
  if (!snapshot.backfillDone || !snapshot.digestDone) {
    throw new Error(`unexpected livewire quiet background state: ${JSON.stringify(snapshot)}`);
  }
}

function assertLivewireDetailReadySnapshot(snapshot) {
  if (!snapshot.ready || snapshot.view !== 'detail' || !snapshot.detailVisible || !snapshot.detailReady) {
    throw new Error(`livewire detail snapshot not ready: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.detailId !== LIVEWIRE_TARGET_ID || snapshot.detailTitle !== snapshot.targetCardTitle || typeof snapshot.detailOwner !== 'string' || snapshot.detailOwner.length < 3 || snapshot.detailCommentCount !== 3) {
    throw new Error(`unexpected livewire detail metadata: ${JSON.stringify(snapshot)}`);
  }
}

function assertLivewireDetailSettledSnapshot(snapshot) {
  if (!snapshot.ready || !snapshot.settled || snapshot.view !== 'detail' || !snapshot.detailVisible || !snapshot.detailReady) {
    throw new Error(`livewire detail snapshot not settled: ${JSON.stringify(snapshot)}`);
  }
  if (snapshot.detailId !== LIVEWIRE_TARGET_ID || snapshot.detailTitle !== snapshot.targetCardTitle || typeof snapshot.detailOwner !== 'string' || snapshot.detailOwner.length < 3 || snapshot.detailCommentCount !== 3) {
    throw new Error(`unexpected livewire detail settled metadata: ${JSON.stringify(snapshot)}`);
  }
  if (!snapshot.detailChartLoaded) {
    throw new Error(`livewire detail chart not loaded: ${JSON.stringify(snapshot)}`);
  }
}

module.exports = {
  CHROME_PATH,
  CONDUIT_ARTICLE_SLUG,
  CONDUIT_FLOW_COMMENT,
  ITERS,
  LIVE_INTERNET,
  LIVEWIRE_TARGET_ID,
  OPENVERSE_TARGET_ID,
  OPENVERSE_TARGET_TITLE,
  ROOT,
  RWA_AMOUNT,
  RWA_NOTE,
  RWA_RECEIPT_ID,
  RWA_RECIPIENT,
  SIGNALBOARD_TARGET_ID,
  SIGNALBOARD_TARGET_TITLE,
  assertActiveFilteredSnapshot,
  assertCompletedSnapshot,
  assertConduitArticleSnapshot,
  assertConduitFeedSnapshot,
  assertConduitLoginSnapshot,
  assertFinalSnapshot,
  assertInitialSnapshot,
  assertLivewireDetailReadySnapshot,
  assertLivewireDetailSettledSnapshot,
  assertLivewireQuietSnapshot,
  assertLivewireReadySnapshot,
  assertLivewireSettledSnapshot,
  assertOpenverseDetailSnapshot,
  assertOpenverseFilteredSnapshot,
  assertOpenverseInitialSnapshot,
  assertRwaDashboardSnapshot,
  assertRwaLoginSnapshot,
  assertRwaReceiptSnapshot,
  assertRwaReviewSnapshot,
  assertSignalboardDetailReadySnapshot,
  assertSignalboardDetailSettledSnapshot,
  assertSignalboardQuietSnapshot,
  assertSignalboardReadySnapshot,
  assertSignalboardSettledSnapshot,
  conduitUrl,
  livewireUrl,
  openverseUrl,
  printStats,
  rwaUrl,
  stats,
  timed,
  todoMvcUrl,
};
