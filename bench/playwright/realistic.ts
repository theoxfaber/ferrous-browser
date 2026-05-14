// Playwright side of the realistic flow bench.
//
// Run:
//   node realistic.ts
//   bun realistic.ts
const { chromium } = require('playwright');
const {
  CHROME_PATH,
  CONDUIT_ARTICLE_SLUG,
  CONDUIT_FLOW_COMMENT,
  ITERS,
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
} = require('../realistic/common.ts');

async function waitReady(page) {
  await page.waitForFunction(() => document.body.dataset.appReady === 'true', null, { polling: 'raf', timeout: 10000 });
}

async function waitSettled(page) {
  await page.waitForFunction(() => document.body.dataset.uiSettled === 'true', null, { polling: 'raf', timeout: 10000 });
}

async function snapshot(page) {
  return page.evaluate(() => window.__bench.snapshot());
}

async function loadInitialState(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitReady(page);
  await waitSettled(page);
  assertInitialSnapshot(await snapshot(page));
}

async function loadConduitLogin(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitReady(page);
  await waitSettled(page);
  assertConduitLoginSnapshot(await snapshot(page));
}

async function addTodo(page, title) {
  await page.locator('.new-todo').fill(title);
  await page.locator('.add-todo').click();
  await waitSettled(page);
}

async function prepareCompletedView(page) {
  await addTodo(page, 'Capture settled screenshot');
  await addTodo(page, 'Trim flaky setup');
  await page.locator('.todo-list li:last-child .toggle').click();
  await waitSettled(page);
  await page.locator('.filter-completed').click();
  await waitSettled(page);
  assertCompletedSnapshot(await snapshot(page));
}

async function runFullFlow(page) {
  await prepareCompletedView(page);
  await page.locator('.clear-completed').click();
  await waitSettled(page);
  await page.locator('.filter-all').click();
  await waitSettled(page);
  assertFinalSnapshot(await snapshot(page));
}

async function conduitLoginToFeed(page) {
  await page.locator('.login-submit').click();
  await waitSettled(page);
  assertConduitFeedSnapshot(await snapshot(page), 42, false);
}

async function conduitFavoriteComposite(page) {
  await page.locator(`.favorite-button[data-slug="${CONDUIT_ARTICLE_SLUG}"]`).click();
  await waitSettled(page);
  assertConduitFeedSnapshot(await snapshot(page), 43, true);
}

async function conduitOpenCompositeArticle(page) {
  await page.locator(`.open-article[data-slug="${CONDUIT_ARTICLE_SLUG}"]`).click();
  await waitSettled(page);
  assertConduitArticleSnapshot(await snapshot(page), [
    'The timer flush is the whole trick.',
    'Load and quiet are not the same thing.',
  ]);
}

async function conduitPostComment(page, comment) {
  await page.locator('.article-comment-input').fill(comment);
  await page.locator('.article-comment-submit').click();
  await waitSettled(page);
  assertConduitArticleSnapshot(await snapshot(page), [
    comment,
    'The timer flush is the whole trick.',
    'Load and quiet are not the same thing.',
  ]);
}

(async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: CHROME_PATH,
    args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'],
  });
  const context = await browser.newContext();
  const page = await context.newPage();
  const url = todoMvcUrl();
  const conduit = conduitUrl();

  const bootReadySamples = [];
  for (let i = 0; i < ITERS; i++) {
    bootReadySamples.push(await timed(() => loadInitialState(page, url)));
  }
  const todomvcBootReady = stats(bootReadySamples);
  printStats('todomvc_boot_ready', todomvcBootReady);

  const fullFlowSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadInitialState(page, url);
    fullFlowSamples.push(await timed(() => runFullFlow(page)));
  }
  const todomvcFullFlow = stats(fullFlowSamples);
  printStats('todomvc_full_flow', todomvcFullFlow);

  const settledScreenshotSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadInitialState(page, url);
    await prepareCompletedView(page);
    settledScreenshotSamples.push(await timed(async () => {
      await page.locator('.filter-active').click();
      await waitSettled(page);
      assertActiveFilteredSnapshot(await snapshot(page));
      const png = await page.screenshot();
      if (png.length < 10_000) {
        throw new Error(`unexpectedly small screenshot: ${png.length} bytes`);
      }
    }));
  }
  const todomvcSettledScreenshot = stats(settledScreenshotSamples);
  printStats('todomvc_settled_screenshot', todomvcSettledScreenshot);

  const conduitLoginReadySamples = [];
  for (let i = 0; i < ITERS; i++) {
    conduitLoginReadySamples.push(await timed(() => loadConduitLogin(page, conduit)));
  }
  const conduitLoginReady = stats(conduitLoginReadySamples);
  printStats('conduit_login_ready', conduitLoginReady);

  const conduitAuthArticleFlowSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadConduitLogin(page, conduit);
    conduitAuthArticleFlowSamples.push(await timed(async () => {
      await conduitLoginToFeed(page);
      await conduitFavoriteComposite(page);
      await conduitOpenCompositeArticle(page);
      await conduitPostComment(page, CONDUIT_FLOW_COMMENT);
    }));
  }
  const conduitAuthArticleFlow = stats(conduitAuthArticleFlowSamples);
  printStats('conduit_auth_article_flow', conduitAuthArticleFlow);

  const conduitArticleSettledScreenshotSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadConduitLogin(page, conduit);
    await conduitLoginToFeed(page);
    await conduitFavoriteComposite(page);
    conduitArticleSettledScreenshotSamples.push(await timed(async () => {
      await conduitOpenCompositeArticle(page);
      const png = await page.screenshot();
      if (png.length < 15_000) {
        throw new Error(`unexpectedly small conduit screenshot: ${png.length} bytes`);
      }
    }));
  }
  const conduitArticleSettledScreenshot = stats(conduitArticleSettledScreenshotSamples);
  printStats('conduit_article_settled_screenshot', conduitArticleSettledScreenshot);

  console.log(`RESULTS_JSON ${JSON.stringify({
    library: 'playwright',
    scenario: 'realistic',
    metrics: {
      todomvc_boot_ready: todomvcBootReady,
      todomvc_full_flow: todomvcFullFlow,
      todomvc_settled_screenshot: todomvcSettledScreenshot,
      conduit_login_ready: conduitLoginReady,
      conduit_auth_article_flow: conduitAuthArticleFlow,
      conduit_article_settled_screenshot: conduitArticleSettledScreenshot,
    },
  })}`);

  await browser.close();
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
