# Benchmarks

Apples-to-apples benchmarks comparing `ferrous-browser` against Puppeteer,
Playwright, chromiumoxide, and headless_chrome.

## Methodology

Every harness uses **the same Chrome binary** so library overhead is the only
variable. By default each harness looks in the puppeteer cache:

```
$HOME/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome
```

Set the `CHROME_PATH` environment variable to override (e.g. `CHROME_PATH=/usr/bin/chromium cargo run --release`).

Each harness measures the same matrix with the same iteration counts:

| Step                          | n  |
|-------------------------------|----|
| `launch_chrome` (cold)        | 5  |
| `new_page` (warm browser)     | 20 |
| `goto about:blank` (warm)     | 20 |
| `screenshot`                  | 20 |
| `evaluate` (`document.title`) | 20 |
| `wait_for_selector` gap       | 20 |
| `networkidle_static`          | 20 |
| `networkidle_deferred_250`    | 20 |
| `wait_for_function` gap       | 20 |
| `click_when_enabled` gap      | 20 |

The harness reports median and p10 (lower-is-better) so single-shot outliers
don't dominate.

The "`wait_for_selector` gap" measures the time between an element being
inserted into the DOM (recorded via `performance.now()` inside the page) and
the wait helper returning. It isolates each library's reaction strategy
(polling cadence vs. observer) from everything else.

## Running

```sh
# Full median-of-medians matrix (3 runs by default)
cargo run --release --example suite_bench

For ferrous-browser-only work, prefer the direct Rust suite above. It keeps the
parity and realistic lanes in one direct process and avoids the extra
parent-process hop that can make browser startup flaky in constrained
CI/sandbox environments.

For the full cross-library comparison matrix:

node bench/run_matrix.ts

# Opt into Bun-backed Puppeteer / Playwright columns as well
JS_RUNTIMES=node,bun node bench/run_matrix.ts

# ferrous-browser (from the repo root)
cargo run --release --example parity_bench

# Puppeteer
cd bench/puppeteer && npm install && node bench.ts

# Playwright
cd bench/playwright && npm install && node bench.ts

# chromiumoxide
cd bench/chromiumoxide && cargo run --release --bin bench

# headless_chrome
cd bench/headless_chrome && cargo run --release --bin bench
```

Node.js v22.18.0+ can run these `.ts` files directly with native type
stripping. On older Node releases, use `node --experimental-strip-types ...`
or run the same commands with Bun instead.

Single-harness runs range from tens of seconds to several minutes depending on
the library. The full `RUNS=3` matrix is materially longer, especially once
`headless_chrome` is included.

The matrix runner executes every harness `RUNS` times, aggregates the median of
each run's medians, and prints a Markdown table. Set `RUNS=1` for a quick local
smoke pass. By default the JS libraries run under Node only; set
`JS_RUNTIMES=node,bun` to add Bun-backed Puppeteer / Playwright columns.

Current status: Bun-backed realistic-flow runs completed for both Puppeteer and
Playwright, and the parity bench completed for Puppeteer. Playwright's parity
bench still showed Bun-specific instability in this repo's current setup, so
Bun remains opt-in rather than the default matrix runtime.

For the realistic-flow scenario lane, see [`realistic/README.md`](realistic/README.md).

## Results

See the table in the project README under
[**Benchmarks**](../README.md#benchmarks). That section now includes both the
hot-path matrix and the deterministic realistic-flow matrix. The published
numbers are median-of-medians across 3 independent runs on a single Linux host.
