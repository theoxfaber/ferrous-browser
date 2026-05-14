const { spawn } = require('child_process');

const DEFAULT_RETRY_ATTEMPTS = 3;
const RETRY_MIN_DELAY_MS = 150;
const RETRY_MAX_DELAY_MS = 300;
const FERROUS_BROWSER_RETRY_SIGNATURES = [
  {
    needle: 'Browser not launched (devtools port not announced)',
    reason: 'devtools_port_not_announced',
  },
  {
    needle: 'kind: DevtoolsPortNotAnnounced',
    reason: 'devtools_port_not_announced',
  },
  {
    needle: 'Chrome exited before announcing its DevTools port',
    reason: 'devtools_port_not_announced',
  },
  {
    needle: 'Browser not launched (startup timed out)',
    reason: 'startup_timed_out',
  },
  {
    needle: 'kind: StartupTimedOut',
    reason: 'startup_timed_out',
  },
  {
    needle: 'Browser not launched (stderr read failed)',
    reason: 'stderr_read_failed',
  },
  {
    needle: 'kind: StderrReadFailed',
    reason: 'stderr_read_failed',
  },
  {
    needle: 'Browser not launched (connect failed)',
    reason: 'connect_failed',
  },
  {
    needle: 'kind: ConnectFailed',
    reason: 'connect_failed',
  },
];

function commandText(cmd) {
  return cmd.join(' ');
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function retryAttempts() {
  const parsed = Number(process.env.HARNESS_RETRY_ATTEMPTS || DEFAULT_RETRY_ATTEMPTS);
  if (!Number.isFinite(parsed) || parsed < 1) {
    return DEFAULT_RETRY_ATTEMPTS;
  }
  return Math.floor(parsed);
}

function retryDelayMs(retryIndex) {
  return Math.min(RETRY_MIN_DELAY_MS * 2 ** retryIndex, RETRY_MAX_DELAY_MS);
}

function commandFailure(cmd, code, stdout, stderr) {
  const combinedOutput = [stderr, stdout].filter(Boolean).join('\n').trim();
  const message = combinedOutput
    ? `command failed (${code}): ${commandText(cmd)}\n${combinedOutput}`
    : `command failed (${code}): ${commandText(cmd)}`;
  const error = new Error(message);
  error.code = code;
  error.command = cmd;
  error.stdout = stdout;
  error.stderr = stderr;
  return error;
}

function missingResultsFailure(cmd, stdout, stderr) {
  const error = new Error(`missing RESULTS_JSON line from: ${commandText(cmd)}`);
  error.command = cmd;
  error.stdout = stdout;
  error.stderr = stderr;
  return error;
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
    child.stdout.on('data', (chunk) => {
      const text = chunk.toString();
      stdout += text;
      process.stdout.write(text);
    });
    child.stderr.on('data', (chunk) => {
      const text = chunk.toString();
      stderr += text;
      process.stderr.write(text);
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code !== 0) {
        reject(commandFailure(cmd, code, stdout, stderr));
        return;
      }

      const resultsLine = stdout
        .split('\n')
        .find((line) => line.startsWith('RESULTS_JSON '));
      if (!resultsLine) {
        reject(missingResultsFailure(cmd, stdout, stderr));
        return;
      }

      resolve(JSON.parse(resultsLine.slice('RESULTS_JSON '.length)));
    });
  });
}

function classifyRetryableHarnessFailure(harnessName, error) {
  if (harnessName !== 'ferrous-browser') {
    return null;
  }

  const output = [error.message, error.stderr, error.stdout].filter(Boolean).join('\n');
  for (const signature of FERROUS_BROWSER_RETRY_SIGNATURES) {
    if (output.includes(signature.needle)) {
      return signature.reason;
    }
  }
  return null;
}

async function runCommandWithRetry(harness) {
  const maxAttempts = retryAttempts();

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    try {
      return await runCommand(harness.cwd, harness.cmd);
    } catch (error) {
      const reason = classifyRetryableHarnessFailure(harness.name, error);
      if (!reason || attempt === maxAttempts) {
        throw error;
      }

      const delayMs = retryDelayMs(attempt - 1);
      console.warn(
        `[${harness.name}] retrying after ${reason}; attempt ${attempt + 1}/${maxAttempts} in ${delayMs}ms`,
      );
      await sleep(delayMs);
    }
  }

  throw new Error(`retry loop exhausted unexpectedly for ${harness.name}`);
}

module.exports = {
  runCommandWithRetry,
};
