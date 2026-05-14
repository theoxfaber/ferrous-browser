// Puppeteer side of the realistic flow bench.
//
// Run:
//   node realistic.ts
//   bun realistic.ts
const puppeteer = require('puppeteer');
const { startSignalboardServer } = require('../realistic/signalboard_server.ts');
const {
  CHROME_PATH,
  CONDUIT_ARTICLE_SLUG,
  CONDUIT_FLOW_COMMENT,
  ITERS,
  LIVE_INTERNET,
  LIVEWIRE_TARGET_ID,
  OPENVERSE_TARGET_ID,
  RWA_AMOUNT,
  RWA_NOTE,
  RWA_RECIPIENT,
  SIGNALBOARD_TARGET_ID,
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
} = require('../realistic/common.ts');

async function waitReady(page) {
  await page.waitForFunction("document.body.dataset.appReady === 'true'", { polling: 'raf', timeout: 10000 });
}

async function waitSettled(page) {
  await page.waitForFunction("document.body.dataset.uiSettled === 'true'", { polling: 'raf', timeout: 10000 });
}

async function waitNetworkQuiet(page) {
  await page.waitForFunction("document.body.dataset.networkQuiet === 'true'", { polling: 'raf', timeout: 10000 });
}

async function waitDetailReady(page) {
  await page.waitForFunction("document.body.dataset.detailReady === 'true'", { polling: 'raf', timeout: 10000 });
}

async function snapshot(page) {
  return page.evaluate(() => window.__bench.snapshot());
}

let signalboardRunId = 0;
let livewireRunId = 0;

function nextSignalboardUrl(base) {
  const url = new URL(base);
  url.searchParams.set('run', String(signalboardRunId++));
  return url.toString();
}

function nextLivewireUrl(base) {
  const url = new URL(base);
  url.searchParams.set('run', String(livewireRunId++));
  return url.toString();
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

async function loadSignalboardReady(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitReady(page);
  assertSignalboardReadySnapshot(await snapshot(page));
}

async function loadSignalboardSettled(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitSettled(page);
  assertSignalboardSettledSnapshot(await snapshot(page));
}

async function loadSignalboardQuiet(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitNetworkQuiet(page);
  assertSignalboardQuietSnapshot(await snapshot(page));
}

async function loadLivewireReady(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitReady(page);
  assertLivewireReadySnapshot(await snapshot(page));
}

async function loadLivewireSettled(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitSettled(page);
  assertLivewireSettledSnapshot(await snapshot(page));
}

async function loadLivewireQuiet(page, url) {
  await page.goto(url, { waitUntil: 'load' });
  await waitNetworkQuiet(page);
  assertLivewireQuietSnapshot(await snapshot(page));
}

async function addTodo(page, title) {
  await page.type('.new-todo', title);
  await page.click('.add-todo');
  await waitSettled(page);
}

async function prepareCompletedView(page) {
  await addTodo(page, 'Capture settled screenshot');
  await addTodo(page, 'Trim flaky setup');
  await page.click('.todo-list li:last-child .toggle');
  await waitSettled(page);
  await page.click('.filter-completed');
  await waitSettled(page);
  assertCompletedSnapshot(await snapshot(page));
}

async function runFullFlow(page) {
  await prepareCompletedView(page);
  await page.click('.clear-completed');
  await waitSettled(page);
  await page.click('.filter-all');
  await waitSettled(page);
  assertFinalSnapshot(await snapshot(page));
}

async function conduitLoginToFeed(page) {
  await page.click('.login-submit');
  await waitSettled(page);
  assertConduitFeedSnapshot(await snapshot(page), 42, false);
}

async function conduitFavoriteComposite(page) {
  await page.click(`.favorite-button[data-slug="${CONDUIT_ARTICLE_SLUG}"]`);
  await waitSettled(page);
  assertConduitFeedSnapshot(await snapshot(page), 43, true);
}

async function conduitOpenCompositeArticle(page) {
  await page.click(`.open-article[data-slug="${CONDUIT_ARTICLE_SLUG}"]`);
  await waitSettled(page);
  assertConduitArticleSnapshot(await snapshot(page), [
    'The timer flush is the whole trick.',
    'Load and quiet are not the same thing.',
  ]);
}

async function conduitPostComment(page, comment) {
  await page.type('.article-comment-input', comment);
  await page.click('.article-comment-submit');
  await waitSettled(page);
  assertConduitArticleSnapshot(await snapshot(page), [
    comment,
    'The timer flush is the whole trick.',
    'Load and quiet are not the same thing.',
  ]);
}

async function openverseApplyFilters(page) {
  await page.click('.media-image');
  await waitSettled(page);
  await page.click('.license-cc0');
  await waitSettled(page);
  assertOpenverseFilteredSnapshot(await snapshot(page));
}

async function openverseOpenTargetDetail(page) {
  await page.click(`.open-detail[data-id="${OPENVERSE_TARGET_ID}"]`);
  await waitSettled(page);
  assertOpenverseDetailSnapshot(await snapshot(page));
}

async function rwaLoginToDashboard(page) {
  await page.click('.login-submit');
  await waitSettled(page);
  assertRwaDashboardSnapshot(await snapshot(page), false);
}

async function rwaOpenComposer(page) {
  await page.click('.start-payment');
  await waitSettled(page);
  assertRwaDashboardSnapshot(await snapshot(page), true);
}

async function rwaDraftPayment(page) {
  await page.type('.payment-recipient', RWA_RECIPIENT);
  await page.type('.payment-amount', RWA_AMOUNT);
  await page.type('.payment-note', RWA_NOTE);
}

async function rwaReviewPayment(page) {
  await page.click('.payment-review');
  await waitSettled(page);
  assertRwaReviewSnapshot(await snapshot(page));
}

async function rwaSubmitPayment(page) {
  await page.click('.payment-submit');
  await waitSettled(page);
  assertRwaReceiptSnapshot(await snapshot(page));
}

async function signalboardOpenDetail(page) {
  await page.click(`.open-detail[data-id="${SIGNALBOARD_TARGET_ID}"]`);
  await waitDetailReady(page);
  assertSignalboardDetailReadySnapshot(await snapshot(page));
}

async function signalboardWaitDetailSettled(page) {
  await waitSettled(page);
  assertSignalboardDetailSettledSnapshot(await snapshot(page));
}

async function livewireOpenDetail(page) {
  await page.click(`.open-detail[data-id="${LIVEWIRE_TARGET_ID}"]`);
  await waitDetailReady(page);
  assertLivewireDetailReadySnapshot(await snapshot(page));
}

async function livewireWaitDetailSettled(page) {
  await waitSettled(page);
  assertLivewireDetailSettledSnapshot(await snapshot(page));
}

(async () => {
  const signalboardServer = await startSignalboardServer();
  const browser = await puppeteer.launch({
    headless: 'new',
    executablePath: CHROME_PATH,
    args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'],
  });
  const page = await browser.newPage();
  const url = todoMvcUrl();
  const conduit = conduitUrl();
  const openverse = openverseUrl();
  const rwa = rwaUrl();
  const signalboard = signalboardServer.url();
  const livewire = livewireUrl();

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
      await page.click('.filter-active');
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

  const signalboardInteractionReadySamples = [];
  for (let i = 0; i < ITERS; i++) {
    signalboardInteractionReadySamples.push(await timed(() => loadSignalboardReady(page, nextSignalboardUrl(signalboard))));
  }
  const signalboardInteractionReady = stats(signalboardInteractionReadySamples);
  printStats('signalboard_interaction_ready', signalboardInteractionReady);

  const signalboardVisualSettledSamples = [];
  for (let i = 0; i < ITERS; i++) {
    signalboardVisualSettledSamples.push(await timed(() => loadSignalboardSettled(page, nextSignalboardUrl(signalboard))));
  }
  const signalboardVisualSettled = stats(signalboardVisualSettledSamples);
  printStats('signalboard_visual_settled', signalboardVisualSettled);

  const signalboardNetworkQuiescedSamples = [];
  for (let i = 0; i < ITERS; i++) {
    signalboardNetworkQuiescedSamples.push(await timed(() => loadSignalboardQuiet(page, nextSignalboardUrl(signalboard))));
  }
  const signalboardNetworkQuiesced = stats(signalboardNetworkQuiescedSamples);
  printStats('signalboard_network_quiesced', signalboardNetworkQuiesced);

  const signalboardOpenDetailFlowSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadSignalboardSettled(page, nextSignalboardUrl(signalboard));
    signalboardOpenDetailFlowSamples.push(await timed(() => signalboardOpenDetail(page)));
  }
  const signalboardOpenDetailFlow = stats(signalboardOpenDetailFlowSamples);
  printStats('signalboard_open_detail_flow', signalboardOpenDetailFlow);

  const signalboardDetailSettledScreenshotSamples = [];
  for (let i = 0; i < ITERS; i++) {
    await loadSignalboardSettled(page, nextSignalboardUrl(signalboard));
    signalboardDetailSettledScreenshotSamples.push(await timed(async () => {
      await signalboardOpenDetail(page);
      await signalboardWaitDetailSettled(page);
      const png = await page.screenshot();
      if (png.length < 15_000) {
        throw new Error(`unexpectedly small signalboard screenshot: ${png.length}`);
      }
    }));
  }
  const signalboardDetailSettledScreenshot = stats(signalboardDetailSettledScreenshotSamples);
  printStats('signalboard_detail_settled_screenshot', signalboardDetailSettledScreenshot);

  let livewireInteractionReady = null;
  let livewireVisualSettled = null;
  let livewireNetworkQuiesced = null;
  let livewireOpenDetailFlow = null;
  let livewireDetailSettledScreenshot = null;

  if (LIVE_INTERNET) {
    const livewireInteractionReadySamples = [];
    for (let i = 0; i < ITERS; i++) {
      livewireInteractionReadySamples.push(await timed(() => loadLivewireReady(page, nextLivewireUrl(livewire))));
    }
    livewireInteractionReady = stats(livewireInteractionReadySamples);
    printStats('livewire_interaction_ready', livewireInteractionReady);

    const livewireVisualSettledSamples = [];
    for (let i = 0; i < ITERS; i++) {
      livewireVisualSettledSamples.push(await timed(() => loadLivewireSettled(page, nextLivewireUrl(livewire))));
    }
    livewireVisualSettled = stats(livewireVisualSettledSamples);
    printStats('livewire_visual_settled', livewireVisualSettled);

    const livewireNetworkQuiescedSamples = [];
    for (let i = 0; i < ITERS; i++) {
      livewireNetworkQuiescedSamples.push(await timed(() => loadLivewireQuiet(page, nextLivewireUrl(livewire))));
    }
    livewireNetworkQuiesced = stats(livewireNetworkQuiescedSamples);
    printStats('livewire_network_quiesced', livewireNetworkQuiesced);

    const livewireOpenDetailFlowSamples = [];
    for (let i = 0; i < ITERS; i++) {
      await loadLivewireSettled(page, nextLivewireUrl(livewire));
      livewireOpenDetailFlowSamples.push(await timed(() => livewireOpenDetail(page)));
    }
    livewireOpenDetailFlow = stats(livewireOpenDetailFlowSamples);
    printStats('livewire_open_detail_flow', livewireOpenDetailFlow);

    const livewireDetailSettledScreenshotSamples = [];
    for (let i = 0; i < ITERS; i++) {
      await loadLivewireSettled(page, nextLivewireUrl(livewire));
      livewireDetailSettledScreenshotSamples.push(await timed(async () => {
        await livewireOpenDetail(page);
        await livewireWaitDetailSettled(page);
        const png = await page.screenshot();
        if (png.length < 15_000) {
          throw new Error(`unexpectedly small livewire screenshot: ${png.length} bytes`);
        }
      }));
    }
    livewireDetailSettledScreenshot = stats(livewireDetailSettledScreenshotSamples);
    printStats('livewire_detail_settled_screenshot', livewireDetailSettledScreenshot);
  }

  const metrics = {
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
    signalboard_interaction_ready: signalboardInteractionReady,
    signalboard_visual_settled: signalboardVisualSettled,
    signalboard_network_quiesced: signalboardNetworkQuiesced,
    signalboard_open_detail_flow: signalboardOpenDetailFlow,
    signalboard_detail_settled_screenshot: signalboardDetailSettledScreenshot,
  };

  if (LIVE_INTERNET) {
    Object.assign(metrics, {
      livewire_interaction_ready: livewireInteractionReady,
      livewire_visual_settled: livewireVisualSettled,
      livewire_network_quiesced: livewireNetworkQuiesced,
      livewire_open_detail_flow: livewireOpenDetailFlow,
      livewire_detail_settled_screenshot: livewireDetailSettledScreenshot,
    });
  }

  console.log(`RESULTS_JSON ${JSON.stringify({
    library: 'puppeteer',
    scenario: 'realistic',
    metrics,
  })}`);

  await signalboardServer.close();
  await browser.close();
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
