const path = require('path');
const { performance } = require('perf_hooks');
const { pathToFileURL } = require('url');

const ROOT = path.resolve(__dirname, '..', '..');
const ITERS = Number(process.env.ITERS || '10');
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

const CONDUIT_ARTICLE_SLUG = 'composite-network-idle';
const CONDUIT_ARTICLE_TITLE = 'Composite NetworkIdle';
const CONDUIT_FLOW_COMMENT = 'Benchmark the real flow.';

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

module.exports = {
  CHROME_PATH,
  CONDUIT_ARTICLE_SLUG,
  CONDUIT_FLOW_COMMENT,
  ITERS,
  ROOT,
  assertActiveFilteredSnapshot,
  assertCompletedSnapshot,
  assertConduitArticleSnapshot,
  assertConduitFeedSnapshot,
  assertConduitLoginSnapshot,
  assertFinalSnapshot,
  assertInitialSnapshot,
  conduitUrl,
  printStats,
  stats,
  timed,
  todoMvcUrl,
};
