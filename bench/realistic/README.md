# Realistic Flow Benchmarks

This lane complements the hot-path microbenchmarks in [`../README.md`](../README.md).
The goal here is to measure full user-visible flows against deterministic local
fixtures instead of isolated primitives.

## Current scenarios

- `todomvc`: local file-backed TodoMVC-style app with delayed boot, add/toggle/
  filter/clear interactions, and an app-specific settled-screenshot state.
- `conduit`: local file-backed RealWorld-style auth/feed/article app with a
  delayed boot shell, seeded sign-in, favorite propagation, delayed article
  hydration, comment posting, and an article-grade settled screenshot.

## Metrics

Every realistic harness reports the same scenario metrics:

| Metric | Meaning |
|--------|---------|
| `todomvc_boot_ready` | `goto(..., load)` until the app's own ready signal |
| `todomvc_full_flow` | add/toggle/filter/clear flow including correctness checks |
| `todomvc_settled_screenshot` | state change -> app-settled wait -> screenshot |
| `conduit_login_ready` | `goto(..., load)` until the seeded login view is settled |
| `conduit_auth_article_flow` | sign in -> favorite target article -> open it -> post a comment |
| `conduit_article_settled_screenshot` | feed action -> article settle -> screenshot |

The TodoMVC fixture uses `file://` URLs, so every library sees the exact same
HTML, CSS, and JavaScript without a local server sitting in the middle.

## Running

```sh
# Full realistic matrix (3 runs by default)
node bench/run_realistic_matrix.ts

# Opt into Bun-backed Puppeteer / Playwright columns as well
JS_RUNTIMES=node,bun node bench/run_realistic_matrix.ts

# ferrous-browser
cargo run --release --example realistic_bench

# Puppeteer
cd bench/puppeteer && node realistic.ts

# Playwright
cd bench/playwright && node realistic.ts

# chromiumoxide
cd bench/chromiumoxide && cargo run --release --bin realistic

# headless_chrome
cd bench/headless_chrome && cargo run --release --bin realistic
```

Set `RUNS=1` for a quick local smoke pass.
By default the JS libraries run under Node only; set
`JS_RUNTIMES=node,bun` to add Bun-backed Puppeteer / Playwright columns.

Current status: the realistic TodoMVC lane is green under Bun for both
Puppeteer and Playwright. The expanded realistic matrix is still primarily
validated under Node, and the separate hot-path parity bench is only partially
green under Bun today, so Bun support stays opt-in in the runners.

Node.js v22.18.0+ can run these `.ts` files directly with native type
stripping. On older Node releases, use `node --experimental-strip-types ...`
or run the same commands with Bun instead.

## Roadmap

The scaffold is intended to grow into a broader scenario matrix:

- TodoMVC: micro-SPA interaction baseline
- RealWorld / Conduit: auth, routing, CRUD, feed/article flows
- visual-settling target: screenshot-grade loading correctness
- seeded full-stack app: realistic post-login/product flows

## Results

See [README.md](../../README.md#benchmarks) for the current median-of-medians
tables. The public benchmark section now includes both the hot-path parity
matrix and this realistic-flow matrix.
