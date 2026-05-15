#!/usr/bin/env node

const path = require('path');
const { runCommandWithRetry } = require('./matrix_retry');

const ROOT = path.resolve(__dirname, '..');
const RUNS = Number(process.env.RUNS || '3');
const JS_RUNTIMES = (process.env.JS_RUNTIMES || 'node')
  .split(',')
  .map((value) => value.trim())
  .filter(Boolean);
const LIVE_INTERNET = /^(1|true|yes)$/i.test(process.env.LIVE_INTERNET || '');
const HARNESS_FILTER = (process.env.HARNESS_FILTER || '')
  .split(',')
  .map((value) => value.trim())
  .filter(Boolean);

function tsCommand(script, runtime) {
  if (runtime === 'bun') {
    return ['bun', script];
  }

  const [major, minor] = process.versions.node.split('.').map(Number);
  if (major > 22 || (major === 22 && minor >= 18)) {
    return ['node', script];
  }
  return ['node', '--experimental-strip-types', script];
}

const HARNESSES = (() => {
  const harnesses = [
    {
      name: 'ferrous-browser',
      cwd: ROOT,
      cmd: ['cargo', 'run', '--release', '--example', 'realistic_bench'],
    },
  ];

  for (const runtime of JS_RUNTIMES) {
    harnesses.push({
      name: `puppeteer-${runtime}`,
      cwd: path.join(ROOT, 'bench', 'puppeteer'),
      cmd: tsCommand('realistic.ts', runtime),
    });
    harnesses.push({
      name: `playwright-${runtime}`,
      cwd: path.join(ROOT, 'bench', 'playwright'),
      cmd: tsCommand('realistic.ts', runtime),
    });
  }

  harnesses.push(
    {
      name: 'chromiumoxide',
      cwd: path.join(ROOT, 'bench', 'chromiumoxide'),
      cmd: ['cargo', 'run', '--release', '--bin', 'realistic'],
    },
    {
      name: 'headless_chrome',
      cwd: path.join(ROOT, 'bench', 'headless_chrome'),
      cmd: ['cargo', 'run', '--release', '--bin', 'realistic'],
    },
  );

  if (!HARNESS_FILTER.length) {
    return harnesses;
  }

  const allowed = new Set(HARNESS_FILTER);
  return harnesses.filter((harness) => allowed.has(harness.name));
})();

const METRICS = [
  ['todomvc_boot_ready', 'todomvc_boot_ready'],
  ['todomvc_full_flow', 'todomvc_full_flow'],
  ['todomvc_settled_screenshot', 'todomvc_settled_screenshot'],
  ['conduit_login_ready', 'conduit_login_ready'],
  ['conduit_auth_article_flow', 'conduit_auth_article_flow'],
  ['conduit_article_settled_screenshot', 'conduit_article_settled_screenshot'],
  ['openverse_search_ready', 'openverse_search_ready'],
  ['openverse_filter_detail_flow', 'openverse_filter_detail_flow'],
  [
    'openverse_detail_settled_screenshot',
    'openverse_detail_settled_screenshot',
  ],
  ['rwa_login_ready', 'rwa_login_ready'],
  ['rwa_payment_flow', 'rwa_payment_flow'],
  ['rwa_receipt_settled_screenshot', 'rwa_receipt_settled_screenshot'],
  ['signalboard_interaction_ready', 'signalboard_interaction_ready'],
  ['signalboard_visual_settled', 'signalboard_visual_settled'],
  ['signalboard_network_quiesced', 'signalboard_network_quiesced'],
  ['signalboard_open_detail_flow', 'signalboard_open_detail_flow'],
  ['signalboard_detail_settled_screenshot', 'signalboard_detail_settled_screenshot'],
];

if (LIVE_INTERNET) {
  METRICS.push(
    ['livewire_interaction_ready', 'livewire_interaction_ready'],
    ['livewire_visual_settled', 'livewire_visual_settled'],
    ['livewire_network_quiesced', 'livewire_network_quiesced'],
    ['livewire_open_detail_flow', 'livewire_open_detail_flow'],
    ['livewire_detail_settled_screenshot', 'livewire_detail_settled_screenshot'],
  );
}

function median(xs) {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

function formatCell(metric) {
  if (!metric) {
    return 'n/a';
  }
  return `${metric.median.toFixed(1)} ms`;
}

function gcd(a, b) {
  let x = Math.abs(a);
  let y = Math.abs(b);
  while (y !== 0) {
    const t = x % y;
    x = y;
    y = t;
  }
  return x;
}

function harnessRotationStep(length) {
  if (length <= 1) {
    return 1;
  }
  for (let step = Math.floor(length / 2) + 1; step < length; step += 1) {
    if (gcd(step, length) === 1) {
      return step;
    }
  }
  return 1;
}

function orderedHarnessesForRun(runIndex) {
  if (!LIVE_INTERNET || HARNESSES.length <= 1) {
    return HARNESSES;
  }

  const step = harnessRotationStep(HARNESSES.length);
  const offset = (runIndex * step) % HARNESSES.length;
  return HARNESSES.slice(offset).concat(HARNESSES.slice(0, offset));
}

async function main() {
  if (!HARNESSES.length) {
    throw new Error('HARNESS_FILTER excluded every realistic harness');
  }

  const runsByLibrary = {};

  for (const harness of HARNESSES) {
    runsByLibrary[harness.name] = [];
  }

  for (let runIndex = 0; runIndex < RUNS; runIndex += 1) {
    const orderedHarnesses = orderedHarnessesForRun(runIndex);
    if (LIVE_INTERNET) {
      console.log(`\n-- live harness order ${runIndex + 1}/${RUNS}: ${orderedHarnesses.map((h) => h.name).join(', ')}`);
    }
    for (const harness of orderedHarnesses) {
      console.log(`\n== ${harness.name} realistic run ${runIndex + 1}/${RUNS} ==`);
      const result = await runCommandWithRetry(harness);
      runsByLibrary[harness.name].push(result);
    }
  }

  const aggregate = {};
  for (const harness of HARNESSES) {
    const runs = runsByLibrary[harness.name];
    aggregate[harness.name] = {};
    for (const [metricKey] of METRICS) {
      const present = runs
        .map((run) => run.metrics[metricKey])
        .filter((value) => value && typeof value.median === 'number');
      if (!present.length) {
        aggregate[harness.name][metricKey] = null;
        continue;
      }
      aggregate[harness.name][metricKey] = {
        median: median(present.map((value) => value.median)),
        p10: median(present.map((value) => value.p10)),
        n: present[0].n,
        note: present.find((value) => value.note)?.note || null,
      };
    }
  }

  console.log('\nRealistic median-of-medians matrix\n');
  console.log(
    `| Metric                       | ${HARNESSES.map((h) => h.name.padEnd(16)).join(' | ')} |`,
  );
  console.log(
    `| ${'-'.repeat(28)} | ${HARNESSES.map(() => '-'.repeat(16)).join(' | ')} |`,
  );
  for (const [metricKey, label] of METRICS) {
    const row = HARNESSES
      .map((harness) => formatCell(aggregate[harness.name][metricKey]).padEnd(16))
      .join(' | ');
    console.log(`| ${label.padEnd(28)} | ${row} |`);
  }

  const notes = [];
  for (const harness of HARNESSES) {
    for (const [metricKey] of METRICS) {
      const metric = aggregate[harness.name][metricKey];
      if (metric?.note) {
        notes.push(`${harness.name} ${metricKey}: ${metric.note}`);
      }
    }
  }
  if (notes.length) {
    console.log('\nNotes');
    for (const note of notes) {
      console.log(`- ${note}`);
    }
  }

  console.log(`\nRESULTS_JSON ${JSON.stringify({ runs: RUNS, aggregate })}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
