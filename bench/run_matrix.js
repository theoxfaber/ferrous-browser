#!/usr/bin/env node

const { spawn } = require('child_process');
const path = require('path');

const ROOT = path.resolve(__dirname, '..');
const RUNS = Number(process.env.RUNS || '3');

const HARNESSES = [
  {
    name: 'ferrous-browser',
    cwd: ROOT,
    cmd: ['cargo', 'run', '--release', '--example', 'parity_bench'],
  },
  {
    name: 'puppeteer',
    cwd: path.join(ROOT, 'bench', 'puppeteer'),
    cmd: ['node', 'bench.js'],
  },
  {
    name: 'playwright',
    cwd: path.join(ROOT, 'bench', 'playwright'),
    cmd: ['node', 'bench.js'],
  },
  {
    name: 'chromiumoxide',
    cwd: path.join(ROOT, 'bench', 'chromiumoxide'),
    cmd: ['cargo', 'run', '--release'],
  },
  {
    name: 'headless_chrome',
    cwd: path.join(ROOT, 'bench', 'headless_chrome'),
    cmd: ['cargo', 'run', '--release'],
  },
];

const METRICS = [
  ['launch_chrome', 'launch_chrome'],
  ['new_page', 'new_page'],
  ['goto_about_blank', 'goto_about_blank'],
  ['screenshot', 'screenshot'],
  ['evaluate', 'evaluate'],
  ['wait_for_selector_gap', 'wait_for_selector_gap'],
  ['networkidle_static', 'networkidle_static'],
  ['networkidle_deferred_250', 'networkidle_deferred_250'],
  ['wait_for_function_gap', 'wait_for_function_gap'],
  ['click_when_enabled_gap', 'click_when_enabled_gap'],
];

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

function formatMetricName(name) {
  return `| ${name.padEnd(25)} `;
}

function runCommand(cwd, cmd) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd[0], cmd.slice(1), {
      cwd,
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });

    let stdout = '';
    let stderr = '';
    child.stdout.on('data', chunk => {
      const text = chunk.toString();
      stdout += text;
      process.stdout.write(text);
    });
    child.stderr.on('data', chunk => {
      const text = chunk.toString();
      stderr += text;
      process.stderr.write(text);
    });
    child.on('error', reject);
    child.on('close', code => {
      if (code !== 0) {
        reject(new Error(`command failed (${code}): ${cmd.join(' ')}\n${stderr}`));
        return;
      }
      const resultsLine = stdout
        .split('\n')
        .find(line => line.startsWith('RESULTS_JSON '));
      if (!resultsLine) {
        reject(new Error(`missing RESULTS_JSON line from: ${cmd.join(' ')}`));
        return;
      }
      resolve(JSON.parse(resultsLine.slice('RESULTS_JSON '.length)));
    });
  });
}

async function main() {
  const runsByLibrary = {};

  for (const harness of HARNESSES) {
    runsByLibrary[harness.name] = [];
    for (let i = 0; i < RUNS; i++) {
      console.log(`\n== ${harness.name} run ${i + 1}/${RUNS} ==`);
      const result = await runCommand(harness.cwd, harness.cmd);
      runsByLibrary[harness.name].push(result);
    }
  }

  const aggregate = {};
  for (const harness of HARNESSES) {
    const runs = runsByLibrary[harness.name];
    aggregate[harness.name] = {};
    for (const [metricKey] of METRICS) {
      const present = runs
        .map(run => run.metrics[metricKey])
        .filter(value => value && typeof value.median === 'number');
      if (!present.length) {
        aggregate[harness.name][metricKey] = null;
        continue;
      }
      aggregate[harness.name][metricKey] = {
        median: median(present.map(value => value.median)),
        p10: median(present.map(value => value.p10)),
        n: present[0].n,
        note: present.find(value => value.note)?.note || null,
      };
    }
  }

  console.log('\nMedian-of-medians matrix\n');
  console.log(
    `| Metric                     | ${HARNESSES.map(h => h.name.padEnd(16)).join(' | ')} |`
  );
  console.log(
    `| ${'-'.repeat(25)} | ${HARNESSES.map(() => '-'.repeat(16)).join(' | ')} |`
  );
  for (const [metricKey, label] of METRICS) {
    const row = HARNESSES
      .map(harness => formatCell(aggregate[harness.name][metricKey]).padEnd(16))
      .join(' | ');
    console.log(`| ${label.padEnd(25)} | ${row} |`);
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

main().catch(err => {
  console.error(err);
  process.exit(1);
});
