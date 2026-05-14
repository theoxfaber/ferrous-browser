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
- `openverse`: local file-backed Openverse-style search/detail app with a
  delayed media shell, deterministic filter interactions, delayed detail
  hydration, and a screenshot-oriented settled state that makes early capture
  obvious.
- `rwa`: local file-backed Cypress Real World App-style seeded payment flow
  with delayed login/dashboard transitions, deterministic review/receipt
  states, and a receipt-grade settled screenshot.

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
| `openverse_search_ready` | `goto(..., load)` until the seeded search results view is settled |
| `openverse_filter_detail_flow` | apply filters -> open delayed detail view -> assert hydrated media state |
| `openverse_detail_settled_screenshot` | results action -> detail settle -> screenshot |
| `rwa_login_ready` | `goto(..., load)` until the seeded login screen is settled |
| `rwa_payment_flow` | login -> open composer -> draft -> review -> submit payment |
| `rwa_receipt_settled_screenshot` | payment submit -> receipt settle -> screenshot |

All fixtures use `file://` URLs, so every library sees the exact same HTML,
CSS, and JavaScript without a local server sitting in the middle.

## Running

```sh
# Full realistic matrix (3 runs by default)
cargo run --release --example suite_bench

For ferrous-browser-only realistic work, prefer the direct Rust suite above. It
keeps the ferrous lane on the stable direct launch path in constrained
CI/sandbox environments.

For the full cross-library realistic matrix:

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

Set `RUNS=1` for a quick matrix smoke pass, or `ITERS=1` when invoking a
single harness directly.
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

The scaffold is intended to grow into a broader scenario matrix.
All four bullets below are implemented today. The point of calling them out
explicitly is to make the intended benchmark corpus legible to someone new
coming along, rather than leaving them as vague future categories:

- TodoMVC: micro-SPA interaction baseline
- RealWorld / Conduit: auth, routing, CRUD, feed/article flows
- Openverse-style visual-settling target: screenshot-grade loading correctness
  on a media/search page where firing too early is visually obvious
- Cypress Real World App-style seeded full-stack app: realistic post-login /
  dashboard / transaction-style flows with stable local test data

## Results

See [README.md](../../README.md#benchmarks) for the current median-of-medians
tables. The public benchmark section now includes both the hot-path parity
matrix and this realistic-flow matrix.
