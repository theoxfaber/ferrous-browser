// Playwright side of the parity bench. Same shape as puppeteer/bench.js.
//
// Run:
//   node playwright_bench.js
const { chromium } = require('playwright');
const { performance } = require('perf_hooks');

const CHROME_PATH = '/home/claude/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome';
const ITERS = 20;

function median(xs) { const s = [...xs].sort((a,b)=>a-b); return s[Math.floor(s.length/2)]; }
function p10(xs)    { const s = [...xs].sort((a,b)=>a-b); return s[Math.floor(s.length*0.1)]; }
async function timed(fn) { const t0 = performance.now(); await fn(); return performance.now() - t0; }

(async () => {
  const launchArgs = ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'];

  // 1. cold launch
  const cold = [];
  for (let i = 0; i < 5; i++) {
    const t = performance.now();
    const b = await chromium.launch({ headless: true, executablePath: CHROME_PATH, args: launchArgs });
    cold.push(performance.now() - t);
    await b.close();
    await new Promise(r => setTimeout(r, 500));
  }
  console.log(`launch_chrome           median=${median(cold).toFixed(1)}ms  p10=${p10(cold).toFixed(1)}ms  (n=${cold.length})`);

  const browser = await chromium.launch({ headless: true, executablePath: CHROME_PATH, args: launchArgs });
  // Playwright wants a context; use a fresh one to keep methodology consistent.
  const context = await browser.newContext();

  // 2. new_page
  const np = [];
  for (let i = 0; i < ITERS; i++) {
    np.push(await timed(async () => { const p = await context.newPage(); await p.close(); }));
  }
  console.log(`new_page                median=${median(np).toFixed(1)}ms  p10=${p10(np).toFixed(1)}ms  (n=${ITERS})`);

  // 3. goto about:blank
  const page = await context.newPage();
  await page.goto('about:blank', { waitUntil: 'load' });
  const gt = [];
  for (let i = 0; i < ITERS; i++) {
    gt.push(await timed(() => page.goto('about:blank', { waitUntil: 'load' })));
  }
  console.log(`goto about:blank        median=${median(gt).toFixed(1)}ms  p10=${p10(gt).toFixed(1)}ms  (n=${ITERS})`);

  // 4. screenshot
  const ss = [];
  for (let i = 0; i < ITERS; i++) ss.push(await timed(() => page.screenshot()));
  console.log(`screenshot              median=${median(ss).toFixed(1)}ms  p10=${p10(ss).toFixed(1)}ms  (n=${ITERS})`);

  // 5. evaluate
  const ev = [];
  for (let i = 0; i < ITERS; i++) ev.push(await timed(() => page.evaluate(() => document.title)));
  console.log(`evaluate                median=${median(ev).toFixed(2)}ms p10=${p10(ev).toFixed(2)}ms (n=${ITERS})`);

  // 6. wait_for_selector reaction gap
  const html = `<html><body><script>
    window.__injectedAt = null;
    setTimeout(() => {
      const d = document.createElement('div'); d.id = 'target';
      document.body.appendChild(d);
      window.__injectedAt = performance.now();
    }, 200);
  </script></body></html>`;
  const dataUrl = 'data:text/html,' + encodeURIComponent(html);

  const gaps = [];
  for (let i = 0; i < ITERS; i++) {
    await page.goto(dataUrl, { waitUntil: 'load' });
    await page.locator('#target').waitFor({ state: 'attached' });
    const returnedAt = await page.evaluate(() => performance.now());
    const injectedAt = await page.evaluate(() => window.__injectedAt);
    gaps.push(returnedAt - injectedAt);
  }
  console.log(`wait_for_selector gap   median=${median(gaps).toFixed(2)}ms p10=${p10(gaps).toFixed(2)}ms (n=${ITERS})`);

  await browser.close();
})();
