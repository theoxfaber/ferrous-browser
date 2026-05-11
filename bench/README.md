# Benchmarks

Apples-to-apples benchmarks comparing `ferrous-browser` against Puppeteer,
Playwright, and chromiumoxide.

## Methodology

Every harness uses **the same Chrome binary** so library overhead is the only
variable. Adjust `CHROME_PATH` near the top of each script if yours lives
elsewhere; by default they look in the puppeteer cache:

```
/home/<user>/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome
```

Each harness measures the same six operations with the same iteration counts:

| Step                          | n  |
|-------------------------------|----|
| `launch_chrome` (cold)        | 5  |
| `new_page` (warm browser)     | 20 |
| `goto about:blank` (warm)     | 20 |
| `screenshot`                  | 20 |
| `evaluate` (`document.title`) | 20 |
| `wait_for_selector` gap       | 20 |

The harness reports median and p10 (lower-is-better) so single-shot outliers
don't dominate.

The "`wait_for_selector` gap" measures the time between an element being
inserted into the DOM (recorded via `performance.now()` inside the page) and
the wait helper returning. It isolates each library's reaction strategy
(polling cadence vs. observer) from everything else.

## Running

```sh
# ferrous-browser (from the repo root)
cargo run --release --example parity_bench

# Puppeteer (Node ≥18)
cd bench/puppeteer && npm install && node bench.js

# Playwright (Node ≥18)
cd bench/playwright && npm install && node bench.js

# chromiumoxide
cd bench/chromiumoxide && cargo run --release
```

Each run takes 30–90 seconds depending on the library.

## Results

See the table in the project README under
[**Benchmarks**](../README.md#benchmarks). Numbers there are
median-of-medians across 3 independent runs on a single Linux host.
