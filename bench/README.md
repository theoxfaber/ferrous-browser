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
node bench/run_matrix.js

# ferrous-browser (from the repo root)
cargo run --release --example parity_bench

# Puppeteer (Node ≥18)
cd bench/puppeteer && npm install && node bench.js

# Playwright (Node ≥18)
cd bench/playwright && npm install && node bench.js

# chromiumoxide
cd bench/chromiumoxide && cargo run --release

# headless_chrome
cd bench/headless_chrome && cargo run --release
```

Each run takes 30–90 seconds depending on the library.

The matrix runner executes every harness `RUNS` times, aggregates the median of
each run's medians, and prints a Markdown table. Set `RUNS=1` for a quick local
smoke pass.

## Results

See the table in the project README under
[**Benchmarks**](../README.md#benchmarks). Numbers there are
median-of-medians across 3 independent runs on a single Linux host.
