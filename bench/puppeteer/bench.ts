// Puppeteer side of the parity bench.
//
// Run:
//   node bench.ts
//   bun bench.ts
const puppeteer = require('puppeteer');
const { performance } = require('perf_hooks');

const CHROME_PATH = process.env.CHROME_PATH
  || `${process.env.HOME}/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome`;
const ITERS = 20;
const WARMUP_ITERS = 3;
const DELAY_CYCLE_MS = [200, 210, 220, 230, 240];

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
  const base = `${name.padEnd(24)} median=${s.median.toFixed(2)}ms p10=${s.p10.toFixed(2)}ms (n=${s.n})`;
  console.log(s.note ? `${base} [${s.note}]` : base);
}

async function timed(fn) {
  const t0 = performance.now();
  await fn();
  return performance.now() - t0;
}

function selectorGapHtml() {
  return `<html><body><script>
    window.__injectedAt = null;
    setTimeout(() => {
      const d = document.createElement('div'); d.id = 'target';
      document.body.appendChild(d);
      window.__injectedAt = performance.now();
    }, 200);
  </script></body></html>`;
}

function networkidleStaticHtml() {
  return '<!doctype html><html><body>networkidle-static</body></html>';
}

function networkidleDeferredHtml() {
  return `<!doctype html><html><body><script>
    setTimeout(() => fetch('data:text/plain,deferred'), 250);
  </script></body></html>`;
}

function waitForFunctionHtml(delayMs) {
  return `<!doctype html><html><body><script>
    window.__condValue = false;
    window.__condAt = null;
    setTimeout(() => {
      window.__condValue = true;
      window.__condAt = performance.now();
    }, ${delayMs});
  </script></body></html>`;
}

function clickWhenEnabledHtml(delayMs) {
  return `<!doctype html><html><body>
    <button id="btn" disabled>click me</button>
    <script>
      window.__enabledAt = null;
      window.__clickedAt = null;
      document.getElementById('btn').addEventListener('click', () => {
        window.__clickedAt = performance.now();
      });
      setTimeout(() => {
        document.getElementById('btn').disabled = false;
        window.__enabledAt = performance.now();
      }, ${delayMs});
    </script>
  </body></html>`;
}

async function benchNetworkIdle(page, html) {
  const dataUrl = 'data:text/html,' + encodeURIComponent(html);
  for (let i = 0; i < WARMUP_ITERS; i++) {
    await page.goto(dataUrl, { waitUntil: 'networkidle0' });
  }
  const xs = [];
  for (let i = 0; i < ITERS; i++) {
    xs.push(await timed(() => page.goto(dataUrl, { waitUntil: 'networkidle0' })));
  }
  return stats(xs);
}

(async () => {
  const launchArgs = ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'];

  const cold = [];
  for (let i = 0; i < 5; i++) {
    const t = performance.now();
    const browser = await puppeteer.launch({
      headless: 'new',
      executablePath: CHROME_PATH,
      args: launchArgs,
    });
    cold.push(performance.now() - t);
    await browser.close();
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  const launch = stats(cold);
  printStats('launch_chrome', launch);

  const browser = await puppeteer.launch({
    headless: 'new',
    executablePath: CHROME_PATH,
    args: launchArgs,
  });

  const newPageSamples = [];
  for (let i = 0; i < ITERS; i++) {
    newPageSamples.push(await timed(async () => {
      const page = await browser.newPage();
      await page.close();
    }));
  }
  const newPage = stats(newPageSamples);
  printStats('new_page', newPage);

  const page = await browser.newPage();
  await page.goto('about:blank', { waitUntil: 'load' });

  const gotoSamples = [];
  for (let i = 0; i < ITERS; i++) {
    gotoSamples.push(await timed(() => page.goto('about:blank', { waitUntil: 'load' })));
  }
  const gotoAboutBlank = stats(gotoSamples);
  printStats('goto_about_blank', gotoAboutBlank);

  const screenshotSamples = [];
  for (let i = 0; i < ITERS; i++) {
    screenshotSamples.push(await timed(() => page.screenshot()));
  }
  const screenshot = stats(screenshotSamples);
  printStats('screenshot', screenshot);

  const evaluateSamples = [];
  for (let i = 0; i < ITERS; i++) {
    evaluateSamples.push(await timed(() => page.evaluate(() => document.title)));
  }
  const evaluate = stats(evaluateSamples);
  printStats('evaluate', evaluate);

  const selectorUrl = 'data:text/html,' + encodeURIComponent(selectorGapHtml());
  const selectorGaps = [];
  for (let i = 0; i < ITERS; i++) {
    await page.goto(selectorUrl, { waitUntil: 'load' });
    await page.waitForSelector('#target');
    const returnedAt = await page.evaluate(() => performance.now());
    const injectedAt = await page.evaluate(() => window.__injectedAt);
    selectorGaps.push(returnedAt - injectedAt);
  }
  const waitForSelectorGap = stats(selectorGaps);
  printStats('wait_for_selector_gap', waitForSelectorGap);

  const networkidleStatic = await benchNetworkIdle(page, networkidleStaticHtml());
  printStats('networkidle_static', networkidleStatic);

  const networkidleDeferred250 = await benchNetworkIdle(page, networkidleDeferredHtml());
  printStats('networkidle_deferred_250', networkidleDeferred250);

  const waitForFunctionGaps = [];
  for (let i = 0; i < ITERS; i++) {
    const delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.length];
    const dataUrl = 'data:text/html,' + encodeURIComponent(waitForFunctionHtml(delay));
    await page.goto(dataUrl, { waitUntil: 'load' });
    await page.waitForFunction('window.__condValue === true', { polling: 'raf', timeout: 10000 });
    const returnedAt = await page.evaluate(() => performance.now());
    const condAt = await page.evaluate(() => window.__condAt);
    waitForFunctionGaps.push(returnedAt - condAt);
  }
  const waitForFunctionGap = stats(waitForFunctionGaps);
  printStats('wait_for_function_gap', waitForFunctionGap);

  const clickWhenEnabledGaps = [];
  for (let i = 0; i < ITERS; i++) {
    const delay = DELAY_CYCLE_MS[i % DELAY_CYCLE_MS.length];
    const dataUrl = 'data:text/html,' + encodeURIComponent(clickWhenEnabledHtml(delay));
    await page.goto(dataUrl, { waitUntil: 'load' });
    await page.locator('#btn').click();
    const clickedAt = await page.evaluate(() => window.__clickedAt);
    const enabledAt = await page.evaluate(() => window.__enabledAt);
    clickWhenEnabledGaps.push(clickedAt - enabledAt);
  }
  const clickWhenEnabledGap = stats(clickWhenEnabledGaps);
  printStats('click_when_enabled_gap', clickWhenEnabledGap);

  console.log(`RESULTS_JSON ${JSON.stringify({
    library: 'puppeteer',
    metrics: {
      launch_chrome: launch,
      new_page: newPage,
      goto_about_blank: gotoAboutBlank,
      screenshot,
      evaluate,
      wait_for_selector_gap: waitForSelectorGap,
      networkidle_static: networkidleStatic,
      networkidle_deferred_250: networkidleDeferred250,
      wait_for_function_gap: waitForFunctionGap,
      click_when_enabled_gap: clickWhenEnabledGap,
    },
  })}`);

  await browser.close();
})().catch((err) => {
  console.error(err);
  process.exit(1);
});
