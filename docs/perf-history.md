# Wait-primitive perf history

Historical record of how ferrous-browser's wait primitives evolved from
"poll until something happens" to "wait on the exact event Chrome already
emits." Kept here so future readers can see *what we tried, what worked,
what didn't, and why* — the README is for users; this is for whoever needs
the design context to change one of these primitives.

For the user-facing API and headline numbers, see [README.md](../README.md)
under **Benchmarks**.

## Principle

Whenever the library waits, ask: *does Chrome already know the answer?* If
yes, subscribe to the exact CDP event or push the wait into the page itself
via a Promise — don't poll. Magic numbers (`sleep(50ms)`, `quiet for 500ms`)
are last-resort heuristics, not load-bearing infrastructure. Each one we
removed turned a polling-cadence p90 into a frame- or event-bounded p90.

## Timeline

### Change 1 — `WaitUntil::NetworkIdle` resettable `Sleep`

`page.rs` previously selected on `recv() | sleep(50ms)`, checking whether
`last_activity.elapsed() >= 500ms` on each tick. With *any* CDP event on
the broadcast (including events from other sessions) the in-flight sleep
got cancelled and recreated. We replaced it with a single pinned
`tokio::time::Sleep` reset to `now + 500ms` only on events that match the
page's `session_id`.

| | median | σ |
|---|---:|---:|
| before | 525 ms | 1.8 ms |
| after  | 514 ms | 1.5 ms |

11 ms isn't dramatic on its own, but the change was structurally right and
prevented the old code from being measurably perturbed by activity on
other targets.

### Change 2 — `Page::wait_for_function`

Users previously had to do their own Rust-side `evaluate` loop with a
sleep between polls. We added a single in-page `Promise` driven by
`requestAnimationFrame` and awaited via `Runtime.evaluate(awaitPromise:
true)`. Reaction latency is one frame (≤16 ms), not a polling cadence.

| | median | p90 | σ |
|---|---:|---:|---:|
| Rust-side poll(50ms) | 28 ms | 47 ms | 15 ms |
| `wait_for_function`  | 10 ms | 17 ms |  5 ms |

The p90 drop matters more than the median — under realistic load, the
worst-case reaction latency is what stacks up across a test suite.

### Change 3 — `Locator::click_auto`

Adds Playwright-style actionability checks (attached, not disabled, visible,
non-zero box) before the click, all inside one Promise driven by a
`MutationObserver`. The MO callback fires as a *microtask* immediately after
the blocking attribute mutation, so the gap between "element becomes
clickable" and "click event fires" is essentially zero.

| | median | p90 | σ |
|---|---:|---:|---:|
| user-side `wait_for(sel) + poll(disabled) + click` | 30 ms  | 48 ms  | 15 ms |
| `click_auto`                                       | 0.5 ms | 0.6 ms | 0.1 ms |

~60× speedup. The σ collapse from 15 ms to 0.1 ms is what users feel:
clicks no longer have polling-cadence jitter.

### Change 4 — `Page.lifecycleEvent: networkIdle` *(rejected)*

Hypothesis: Chrome already computes "network idle" internally and emits it
as `Page.lifecycleEvent` with `name = "networkIdle"`, so we could just
subscribe to that and delete our timer entirely.

Reality: lifecycle `networkIdle` / `networkAlmostIdle` are
**page-load-metrics signals** tied to Web-Vitals "page is truly settled"
heuristics, not 500ms-quiet-rule events. `examples/lifecycle_probe.rs`
showed both names fired at **~2010 ms** after navigation, regardless of
whether the page had zero fetches or three inline ones — a ~4× regression.

Reverted. The lesson: a CDP event that *sounds* like what you want isn't
necessarily it. Puppeteer's `networkidle0` / `networkidle2`, despite the
name, don't actually use this event either — they count
`Network.requestWillBeSent` / `loadingFinished` / `loadingFailed` in their
own counter.

### Change 5 + 6 — composite `WaitUntil::NetworkIdle`

Combines the lessons from C1–C4. The composite signal:

1. Wait for `Page.loadEventFired` (HTML-discovered resources done).
2. Wait for `Network.*` in-flight counter == 0 (tracked from
   `requestWillBeSent` / `loadingFinished` / `loadingFailed` events,
   filtered by session_id).
3. Force an animation frame to drain microtasks & let any post-load
   work surface, *racing* `requestAnimationFrame` against a raw
   `setTimeout(50)` fallback (see [the rAF-throttling story below](#real-bug-1-background-tab-raf-throttling)).
4. Re-check counter. If still 0 *and* `window.__ferrousPending == 0`
   (set by the document_start setTimeout/clearTimeout wrapper), return.
5. If anything resurfaced during the flush, loop back to step 2.

For trivial pages this returns in ~20 ms instead of ~515 ms. For pages
with chained or `setTimeout`-deferred work, total time scales with the
page's actual work plus ~30–50 ms of fixed overhead — not with a magic
500 ms ceiling.

| workload                | pre-C5 | C5 (no timer wrap) | C6 (with wrap) |
|---|---:|---:|---:|
| A1-static               | 516 ms | 17 ms              | 21 ms          |
| A2-single-wave          | 515 ms | 64 ms              | 65 ms          |
| A3-chained              | 516 ms | 65 ms              | 66 ms          |
| A4-deferred-250         | 516 ms | 17 ms ✗ (early)    | 314 ms ✓       |

The C5 column's A4 ✗ is the false-positive case that motivated C6: without
the document_start wrapper, the composite signal couldn't see a pending
`setTimeout` and returned before it fired.

The wrapper is ~25 lines of injected JS that maintains `__ferrousPending`
and an `__ferrousAwaitTimers()` Promise that resolves on the next moment
the counter hits zero. The composite signal awaits it as the final
condition. See `Page::ensure_timer_script_injected` in `src/page.rs`.

## Battering — what the test suite found

`tests/composite_idle.rs` (26 tests) is the regression gate. It also serves
as the record of what *doesn't* work and why.

### Real bug 1: background-tab rAF throttling

Symptom: with 2+ concurrent `goto(NetworkIdle)` calls, all but one hung
for the full 30 s outer timeout. Cause: headless Chrome pauses
`requestAnimationFrame` on backgrounded tabs, so the composite flush
awaited a frame that never came.

Fix: two-part, both required.

1. `src/browser.rs` adds `--disable-background-timer-throttling`,
   `--disable-renderer-backgrounding`,
   `--disable-backgrounding-occluded-windows` to default args. Same flags
   Puppeteer / Playwright ship; safe for automation.
2. The rAF flush JS races `requestAnimationFrame(r)` against
   `__ferrousRawSetTimeout(r, 50)`. The raw (unwrapped) `setTimeout` is
   captured by the document_start wrapper and exposed so this fallback
   doesn't bump the user-visible pending counter. Belt-and-suspenders so
   even a user who removes the throttling flags can't hang the wait.

### Real bug 2: SSE pinned the in-flight counter

Symptom: a page that opens `new EventSource('/sse')` would hang
`goto(NetworkIdle)` for the full 30 s timeout. Cause: SSE is a regular
HTTP request that streams indefinitely; `Network.requestWillBeSent` fires
but `Network.loadingFinished` doesn't until the connection closes.

Fix: in the composite `update()`, skip requests whose `params.type ==
"EventSource"` at the `requestWillBeSent` event. WebSocket *also* uses a
persistent connection but Chrome emits `Network.webSocketCreated` (a
different event) for it, so it already doesn't pin the counter — verified
by `t2_websocket_known_gap`. Regression gate: `t2_sse_does_not_pin`
(asserts goto returns in <2 s).

### Real bug 3: CDP disconnect mid-goto

Symptom: dropping the `Browser` while a `goto` was in flight made the
goto wait its full 30 s timeout. Cause: `CDPClient` is held in `Arc` by
every `Page`, so dropping the `Browser` doesn't drop the
`broadcast::Sender` — `RecvError::Closed` never fires on the event
broadcast even after Chrome exits. The wait loops silently looped on
"no event yet" forever.

Fix: add a separate `tokio::sync::watch::Sender<bool>` to `CDPClient`
that latches `false → true` when `Connection::run` calls
`fail_all_pending` at WebSocket teardown. All four `goto` wait paths
subscribe via `cdp.disconnected()` and select on
`disconnect_rx.changed()` alongside their normal event recv. Regression
gate: `t3_cdp_disconnect_midgoto` (asserts disconnect surfaces in <3 s
rather than the previous 30 s).

### Known limitations (documented as test assertions)

These are present in the public surface; each has a test asserting current
behaviour so a future change either preserves the contract or flips the
test in the helpful direction.

| Limitation | Test |
|---|---|
| `setInterval` not instrumented (page can issue periodic work invisible to us) | `t1_setinterval_known_gap` |
| Adversarial rebind via `iframe.contentWindow.setTimeout` bypasses our wrapper | `t1_adversarial_rebind_known_gap` |
| iframe-scoped `setTimeout` invisible to top-frame `__ferrousPending` | `t1_iframe_settimeout_known_gap` |

The first two are minor (uncommon patterns in non-adversarial pages). The
third would benefit from a per-frame wrapper aggregation; on the roadmap.

### Hypotheses falsified by the test suite

Listed because each was a real worry going in and the data settled it:

- **WebSocket does NOT pin** the counter (`t2_websocket_known_gap`).
  Chrome emits `Network.webSocketCreated`, not `requestWillBeSent`.
- **No memory leak across 1 000 sequential gotos** (`t3_thousand_sequential_gotos`).
  0 KB RSS growth measured against `/proc/self/status:VmRSS`.
- **Session partitioning is correct** under 5-way parallel gotos
  (`t3_parallel_pages`).
- **Wrapper resets cleanly per-document** — sequential gotos on the same
  Page don't pollute each other's pending state (`t3_sequential_same_page`).

## 2026-05-13 — consolidated benchmark state

This round was not just "run the benches again." It changed both the product
surface and the benchmark rig:

1. **All harnesses now use the same Chrome-for-Testing binary** via
   `CHROME_PATH` / `BrowserConfig.chrome_path`, so the launch numbers are no
   longer polluted by different browser builds.
2. **The experimental wait primitives were promoted to normal surface area**:
   `wait_for_function`, `click_auto`, and the composite `NetworkIdle` path are
   always on, with retryable page/network/script initialization behind them.
3. **The public matrix now has two lanes**:
   hot-path parity (`bench/run_matrix.ts`) and deterministic realistic flows
   (`bench/run_realistic_matrix.ts`).
4. **The realistic fixtures are local and adversarial by design**:
   TodoMVC starts with a visible skeleton and delayed settle signal; Conduit
   adds delayed boot, seeded login, delayed article hydration, favorite state,
   and comment posting. Fast-but-early libraries should fail assertions, not
   just post flattering numbers.

### Current median-of-medians headlines

All numbers below came from `RUNS=3` median-of-medians sweeps on the Linux/CfT
rig described in the README.

| Metric | ferrous-browser | Closest serious comparison | Why it matters |
|---|---:|---:|---|
| `launch_chrome` | 137.3 ms | Playwright 90.4 ms / Puppeteer 161.0 ms | launch is no longer a glaring weakness |
| `new_page` | 13.6 ms | chromiumoxide 22.1 ms | session attach + one-time enable path is paying off |
| `wait_for_selector_gap` | 1.0 ms | Puppeteer 3.5 ms / Playwright 113.0 ms | repeated suite tax stays tiny |
| `networkidle_static` | 19.1 ms | Playwright 503.0 ms / Puppeteer 2016.8 ms | composite idle is in a different behavior class |
| `click_when_enabled_gap` | 0.4 ms | Puppeteer 28.1 ms / Playwright 38.0 ms | in-page actionability removes polling jitter |
| `todomvc_full_flow` | 695.1 ms | Puppeteer 766.6 ms | interaction-heavy realistic flow, not a toy microbench |
| `conduit_auth_article_flow` | 886.4 ms | Puppeteer 964.4 ms | seeded auth/feed/article flow, closer to real E2E shape |

### What changed the benchmark story

- **Launch cost was partly a harness problem.** Earlier ~330 ms ferrous launch
  numbers were taken before the same-binary pin was enforced consistently across
  every library. Once that was corrected and the default Chrome flags were
  tightened for automation, ferrous moved into the same class as the faster
  libraries instead of looking structurally slow.
- **The biggest sustained wins are still event-driven waits.** Selector waits,
  `wait_for_function`, `click_auto`, and composite `NetworkIdle` all improved by
  pushing waits into the page or binding them to exact CDP events instead of
  burning time on host-side polling loops.
- **The realistic lane keeps us honest.** It is now possible to be "fast" in a
  way that is obviously wrong: screenshot the skeleton, miss the delayed article
  body, or race the comment post. The TodoMVC and Conduit fixtures make those
  false wins visible.
- **The comparison set now separates semantic cost from transport cost.**
  Puppeteer and Playwright remain serious baselines. chromiumoxide is useful as
  a Rust-native manual-poll baseline. headless_chrome still completes the flows,
  but its sync transport and polling semantics put it in a different class.

## Reproducing

```bash
# Public hot-path matrix (3-run median-of-medians)
env RUNS=3 node bench/run_matrix.ts

# Public realistic-flow matrix (3-run median-of-medians)
env RUNS=3 node bench/run_realistic_matrix.ts

# Composite NetworkIdle + all wait primitives
cargo test --release --test composite_idle

# Single-library parity lane
cargo run --release --example parity_bench

# Single-library realistic lane
cargo run --release --example realistic_bench

# Correctness verification (every workload reports expected fetch count)
cargo run --release --example idle_verify
```

Local diagnostic harness used during development is in
`examples/lifecycle_probe.rs` (this is the one that disproved C4).
