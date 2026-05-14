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
- `signalboard`: deterministic local HTTP dashboard with staged API fan-out,
  delayed media, background prefetch/audit work, and separate interaction,
  visual, and network-quiet clocks to make methodology tradeoffs legible.
- `livewire` (opt-in): local file-backed shell that performs live browser
  requests directly against public internet endpoints, so the data and media
  fetches are real remote HTTP instead of localhost fixtures or proxying.

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
| `signalboard_interaction_ready` | `goto(..., load)` until the dashboard is usable even though background requests remain |
| `signalboard_visual_settled` | `goto(..., load)` until hero media and visible panels are screenshot-grade stable |
| `signalboard_network_quiesced` | `goto(..., load)` until the initial background insight/prefetch burst fully drains |
| `signalboard_open_detail_flow` | open the target card and wait for the detail panel's content-ready state |
| `signalboard_detail_settled_screenshot` | detail action -> detail settle -> screenshot |
| `livewire_interaction_ready` | `goto(..., load)` until the live remote cards/activity make the page usable |
| `livewire_visual_settled` | `goto(..., load)` until remote images and visible commentary finish settling |
| `livewire_network_quiesced` | `goto(..., load)` until the background live digest chain drains completely |
| `livewire_open_detail_flow` | open the target remote card and wait for the live detail content-ready state |
| `livewire_detail_settled_screenshot` | live detail action -> live detail settle -> screenshot |

Most fixtures use `file://` URLs so every library sees the exact same HTML,
CSS, and JavaScript without a local server sitting in the middle. The
`signalboard` lane is the deliberate exception: it runs against a deterministic
local HTTP server because the point of that scenario is to expose the gap
between interaction readiness, visual settling, and full network quiet.
`livewire` is the other deliberate exception in spirit: the shell is still a
local fixture, but every data/image request goes straight from the browser to
public internet endpoints with cache-busting query params. Keep it opt-in so
the default suite stays deterministic and polite.

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

# Opt into the live-internet scenario as well
LIVE_INTERNET=1 ITERS=1 cargo run --release --example realistic_bench

# Cross-library live-internet smoke
LIVE_INTERNET=1 ITERS=1 RUNS=1 node bench/run_realistic_matrix.ts

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

For `LIVE_INTERNET=1`, prefer `RUNS=3` or more. The matrix runner rotates
harness order between runs so no library is permanently stuck paying the
coldest DNS/TLS/CDN path, and the aggregate table reports median-of-medians
across those independent passes.

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
- Signalboard-style network-heavy dashboard: multi-request, media-heavy startup
  where "user can act", "UI is visually stable", and "the network finally
  went quiet" are intentionally different moments
- Livewire-style live internet dashboard: same clock separation, but with real
  remote HTTP requests leaving the machine instead of deterministic local APIs

## Results

See [README.md](../../README.md#benchmarks) for the current median-of-medians
tables. The public benchmark section now includes the hot-path parity matrix,
this realistic-flow matrix, and the rotated live-internet matrix.
