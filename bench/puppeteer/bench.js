// Puppeteer side of the parity bench.
// All four runners (ferrous, puppeteer, playwright, chromiumoxide) MUST use the
// same Chrome binary and the same warm/cold methodology, otherwise the numbers
// are noise dressed up as competitive analysis.
//
// Run:
//   node puppeteer_bench.js
const puppeteer = require('puppeteer');
const { performance } = require('perf_hooks');

const CHROME_PATH = '/home/claude/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome';
const ITERS = 20;

function median(xs) { const s = [...xs].sort((a,b)=>a-b); return s[Math.floor(s.length/2)]; }
function p10(xs)    { const s = [...xs].sort((a,b)=>a-b); return s[Math.floor(s.length*0.1)]; }

async function timed(fn) {
  const t0 = performance.now();
  await fn();
  return performance.now() - t0;
}

(async () => {
  // ── 1. cold launch ────────────────────────────────────────────────────────
  const cold = [];
  for (let i = 0; i < 5; i++) {
    const t = performance.now();
    const b = await puppeteer.launch({ headless: 'new', executablePath: CHROME_PATH,
      args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'] });
    cold.push(performance.now() - t);
    await b.close();
    await new Promise(r => setTimeout(r, 500));
  }
  console.log(`launch_chrome           median=${median(cold).toFixed(1)}ms  p10=${p10(cold).toFixed(1)}ms  (n=${cold.length})`);

  // One warm browser for all subsequent benches.
  const browser = await puppeteer.launch({ headless: 'new', executablePath: CHROME_PATH,
    args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'] });

  // ── 2. new_page ───────────────────────────────────────────────────────────
  const np = [];
  for (let i = 0; i < ITERS; i++) {
    np.push(await timed(async () => { const p = await browser.newPage(); await p.close(); }));
  }
  console.log(`new_page                median=${median(np).toFixed(1)}ms  p10=${p10(np).toFixed(1)}ms  (n=${ITERS})`);

  // ── 3. goto about:blank (warm page) ──────────────────────────────────────
  const page = await browser.newPage();
  await page.goto('about:blank', { waitUntil: 'load' }); // warmup
  const gt = [];
  for (let i = 0; i < ITERS; i++) {
    gt.push(await timed(() => page.goto('about:blank', { waitUntil: 'load' })));
  }
  console.log(`goto about:blank        median=${median(gt).toFixed(1)}ms  p10=${p10(gt).toFixed(1)}ms  (n=${ITERS})`);

  // ── 4. screenshot ─────────────────────────────────────────────────────────
  const ss = [];
  for (let i = 0; i < ITERS; i++) ss.push(await timed(() => page.screenshot()));
  console.log(`screenshot              median=${median(ss).toFixed(1)}ms  p10=${p10(ss).toFixed(1)}ms  (n=${ITERS})`);

  // ── 5. evaluate ───────────────────────────────────────────────────────────
  const ev = [];
  for (let i = 0; i < ITERS; i++) ev.push(await timed(() => page.evaluate(() => document.title)));
  console.log(`evaluate                median=${median(ev).toFixed(2)}ms p10=${p10(ev).toFixed(2)}ms (n=${ITERS})`);

  // ── 6. wait_for_selector reaction gap ────────────────────────────────────
  // Element injected at performance.now() = T+200ms; measure how soon after
  // injection waitForSelector returns. Pure measurement of polling-vs-observer.
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
    await page.waitForSelector('#target');
    const returnedAt = await page.evaluate(() => performance.now());
    const injectedAt = await page.evaluate(() => window.__injectedAt);
    gaps.push(returnedAt - injectedAt);
  }
  console.log(`wait_for_selector gap   median=${median(gaps).toFixed(2)}ms p10=${p10(gaps).toFixed(2)}ms (n=${ITERS})`);

  await browser.close();
})();
