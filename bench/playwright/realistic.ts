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
  OPENVERSE_TARGET_ID,
  RWA_AMOUNT,
  RWA_NOTE,
  RWA_RECIPIENT,
  assertActiveFilteredSnapshot,
  assertCompletedSnapshot,
  assertConduitArticleSnapshot,
  assertConduitFeedSnapshot,
  assertConduitLoginSnapshot,
  assertFinalSnapshot,
  assertInitialSnapshot,
  assertOpenverseDetailSnapshot,
  assertOpenverseFilteredSnapshot,
  assertOpenverseInitialSnapshot,
  assertRwaDashboardSnapshot,
  assertRwaLoginSnapshot,
  assertRwaReceiptSnapshot,
  assertRwaReviewSnapshot,
  conduitUrl,
  openverseUrl,
  printStats,
  rwaUrl,
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

async function loadOpenverseSearch(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitReady(page);
  await waitSettled(page);
  assertOpenverseInitialSnapshot(await snapshot(page));
}

async function loadRwaLogin(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitReady(page);
  await waitSettled(page);
  assertRwaLoginSnapshot(await snapshot(page));
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

async function openverseApplyFilters(page) {
  await page.locator('.media-image').click();
  await waitSettled(page);
  await page.locator('.license-cc0').click();
  await waitSettled(page);
  assertOpenverseFilteredSnapshot(await snapshot(page));
}

async function openverseOpenTargetDetail(page) {
  await page.locator(`.open-detail[data-id="${OPENVERSE_TARGET_ID}"]`).click();
  await waitSettled(page);
  assertOpenverseDetailSnapshot(await snapshot(page));
}

async function rwaLoginToDashboard(page) {
  await page.locator('.login-submit').click();
  await waitSettled(page);
  assertRwaDashboardSnapshot(await snapshot(page), false);
}

async function rwaOpenComposer(page) {
  await page.locator('.start-payment').click();
  await waitSettled(page);
  assertRwaDashboardSnapshot(await snapshot(page), true);
}

async function rwaDraftPayment(page) {
  await page.locator('.payment-recipient').fill(RWA_RECIPIENT);
  await page.locator('.payment-amount').fill(RWA_AMOUNT);
  await page.locator('.payment-note').fill(RWA_NOTE);
}

async function rwaReviewPayment(page) {
  await page.locator('.payment-review').click();
  await waitSettled(page);
  assertRwaReviewSnapshot(await snapshot(page));
}

async function rwaSubmitPayment(page) {
  await page.locator('.payment-submit').click();
  await waitSettled(page);
  assertRwaReceiptSnapshot(await snapshot(page));
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
  const openverse = openverseUrl();
  const rwa = rwaUrl();

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

  const openverseSearchReadySamples = [];
  for (let i = 0; i < ITERS; i++) {
    openverseSearchReadySamples.push(await timed(() => loadOpenverseSearch(page, openverse)));
  }
  const openverseSearchReady = stats(openverseSearchReadySamples);
  printStats('openverse_search_ready', openverseSearchReady);

  const openverseFilterDetailFlowSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadOpenverseSearch(page, openverse);
    openverseFilterDetailFlowSamples.push(await timed(async () => {
      await openverseApplyFilters(page);
      await openverseOpenTargetDetail(page);
    }));
  }
  const openverseFilterDetailFlow = stats(openverseFilterDetailFlowSamples);
  printStats('openverse_filter_detail_flow', openverseFilterDetailFlow);

  const openverseDetailSettledScreenshotSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadOpenverseSearch(page, openverse);
    await openverseApplyFilters(page);
    openverseDetailSettledScreenshotSamples.push(await timed(async () => {
      await openverseOpenTargetDetail(page);
      const png = await page.screenshot();
      if (png.length < 15_000) {
        throw new Error(`unexpectedly small openverse screenshot: ${png.length} bytes`);
      }
    }));
  }
  const openverseDetailSettledScreenshot = stats(openverseDetailSettledScreenshotSamples);
  printStats('openverse_detail_settled_screenshot', openverseDetailSettledScreenshot);

  const rwaLoginReadySamples = [];
  for (let i = 0; i < ITERS; i++) {
    rwaLoginReadySamples.push(await timed(() => loadRwaLogin(page, rwa)));
  }
  const rwaLoginReady = stats(rwaLoginReadySamples);
  printStats('rwa_login_ready', rwaLoginReady);

  const rwaPaymentFlowSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadRwaLogin(page, rwa);
    rwaPaymentFlowSamples.push(await timed(async () => {
      await rwaLoginToDashboard(page);
      await rwaOpenComposer(page);
      await rwaDraftPayment(page);
      await rwaReviewPayment(page);
      await rwaSubmitPayment(page);
    }));
  }
  const rwaPaymentFlow = stats(rwaPaymentFlowSamples);
  printStats('rwa_payment_flow', rwaPaymentFlow);

  const rwaReceiptSettledScreenshotSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadRwaLogin(page, rwa);
    await rwaLoginToDashboard(page);
    await rwaOpenComposer(page);
    await rwaDraftPayment(page);
    await rwaReviewPayment(page);
    rwaReceiptSettledScreenshotSamples.push(await timed(async () => {
      await rwaSubmitPayment(page);
      const png = await page.screenshot();
      if (png.length < 15_000) {
        throw new Error(`unexpectedly small rwa screenshot: ${png.length} bytes`);
      }
    }));
  }
  const rwaReceiptSettledScreenshot = stats(rwaReceiptSettledScreenshotSamples);
  printStats('rwa_receipt_settled_screenshot', rwaReceiptSettledScreenshot);

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
      openverse_search_ready: openverseSearchReady,
      openverse_filter_detail_flow: openverseFilterDetailFlow,
      openverse_detail_settled_screenshot: openverseDetailSettledScreenshot,
      rwa_login_ready: rwaLoginReady,
      rwa_payment_flow: rwaPaymentFlow,
      rwa_receipt_settled_screenshot: rwaReceiptSettledScreenshot,
    },
  })}`);

  await browser.close();
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
