const fs = require('fs');
const http = require('http');
const path = require('path');

const FIXTURE_ROOT = path.join(__dirname, 'fixtures', 'signalboard');
const INDEX_HTML = fs.readFileSync(path.join(FIXTURE_ROOT, 'index.html'), 'utf8');
const APP_CSS = fs.readFileSync(path.join(FIXTURE_ROOT, 'app.css'), 'utf8');

function noStoreHeaders(contentType) {
  return {
    'content-type': contentType,
    'cache-control': 'no-store, no-cache, must-revalidate',
    pragma: 'no-cache',
    expires: '0',
  };
}

function writeText(res, status, body, contentType) {
  res.writeHead(status, noStoreHeaders(contentType));
  res.end(body);
}

function writeJson(res, body) {
  writeText(res, 200, JSON.stringify(body), 'application/json; charset=utf-8');
}

function delayed(res, ms, writer) {
  setTimeout(() => writer(res), ms);
}

function svgMarkup(title, background, accent) {
  return [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 720 480">',
    '<defs>',
    '<linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">',
    `<stop offset="0%" stop-color="${background}" />`,
    `<stop offset="100%" stop-color="${accent}" />`,
    '</linearGradient>',
    '</defs>',
    '<rect width="720" height="480" fill="url(#bg)" rx="36" ry="36" />',
    '<circle cx="118" cy="114" r="56" fill="rgba(255,255,255,0.18)" />',
    '<path d="M72 352C152 284 248 302 330 248C382 214 450 146 560 164C612 174 650 198 682 224V480H38V396C46 380 58 364 72 352Z" fill="rgba(255,255,255,0.14)" />',
    '<path d="M98 288L188 244L262 268L360 196L454 228L530 178L630 212" fill="none" stroke="rgba(255,255,255,0.9)" stroke-width="14" stroke-linecap="round" stroke-linejoin="round" />',
    `<text x="52" y="420" fill="white" font-family="IBM Plex Sans, Segoe UI, sans-serif" font-size="44" font-weight="700">${title}</text>`,
    '</svg>',
  ].join('');
}

function handleApi(pathname, url, res) {
  if (pathname === '/signalboard/api/bootstrap') {
    return delayed(res, 110, (stream) => writeJson(stream, {
      heroTitle: 'Regional Service Health',
      heroSummary: 'Balancing fan-out, cache pressure, and render latency through the morning traffic ramp.',
      kpis: [
        { label: 'Regions', value: '6' },
        { label: 'Queues', value: '14' },
        { label: 'Edge', value: '98.7%' },
      ],
    }));
  }

  if (pathname === '/signalboard/api/cards') {
    return delayed(res, 180, (stream) => writeJson(stream, {
      cards: [
        {
          id: 'latency-lab',
          title: 'Latency Lab',
          status: 'Watching',
          delta: '+18 ms',
          summary: 'A slow regional fan-out is stretching the render queue after cache misses.',
          cta: 'Open detail',
        },
        {
          id: 'cdn-pulse',
          title: 'CDN Pulse',
          status: 'Stable',
          delta: '-4 ms',
          summary: 'Thumbnail propagation recovered after the overnight purge window.',
          cta: 'Inspect',
        },
        {
          id: 'queue-watch',
          title: 'Queue Watch',
          status: 'Holding',
          delta: '+3 jobs',
          summary: 'Consumer lag is contained, but worker saturation is edging toward the guardrail.',
          cta: 'Inspect',
        },
      ],
    }));
  }

  if (pathname === '/signalboard/api/alerts') {
    return delayed(res, 260, (stream) => writeJson(stream, {
      alerts: [
        {
          title: 'Retry surge',
          summary: 'Cross-region retries lifted 9% after the east cache rewarm.',
        },
        {
          title: 'Thumbnail backlog',
          summary: 'Hero image transforms are draining, but the second wave is still en route.',
        },
      ],
    }));
  }

  if (pathname === '/signalboard/api/activity') {
    return delayed(res, 420, (stream) => writeJson(stream, {
      activity: [
        {
          title: 'Capture lane',
          summary: 'Fresh telemetry is landing on the fast path with only one delayed shard.',
        },
        {
          title: 'Fan-out graph',
          summary: 'The replica spread widened by two regions while the edge rebuilt.',
        },
        {
          title: 'Render queue',
          summary: 'Renderer saturation rose after the batch replay and is easing slowly.',
        },
        {
          title: 'Edge cache',
          summary: 'The top route recovered, but the warmup traffic is still in flight.',
        },
      ],
    }));
  }

  if (pathname === '/signalboard/api/insights') {
    return delayed(res, 1400, (stream) => writeJson(stream, { complete: true }));
  }

  if (pathname === '/signalboard/api/prefetch') {
    return delayed(res, 1800, (stream) => writeJson(stream, { complete: true }));
  }

  if (pathname === '/signalboard/api/detail') {
    return delayed(res, 260, (stream) => writeJson(stream, {
      id: url.searchParams.get('id') || 'latency-lab',
      title: 'Latency Lab',
      owner: 'Runtime Operations',
      summary: 'The render queue is waiting on a slow regional response burst. User-visible controls are ready well before the background audit drains.',
      stages: ['Capture', 'Aggregate', 'Render'],
      auditWindow: 'Background audit closes after the next fan-out sample.',
    }));
  }

  if (pathname === '/signalboard/api/detail-audit') {
    return delayed(res, 900, (stream) => writeJson(stream, { complete: true }));
  }

  return false;
}

function handleAsset(pathname, res) {
  const name = pathname.slice('/signalboard/assets/'.length);
  const assets = {
    'hero-east.svg': { title: 'East fan-out', background: '#0f6f6a', accent: '#4aa89f', delay: 480 },
    'hero-west.svg': { title: 'West queue', background: '#9e4f24', accent: '#d6a04e', delay: 620 },
    'detail-chart.svg': { title: 'Audit trace', background: '#163a55', accent: '#3b7ba8', delay: 380 },
  };
  const asset = assets[name];
  if (!asset) {
    writeText(res, 404, 'not found', 'text/plain; charset=utf-8');
    return true;
  }
  delayed(res, asset.delay, (stream) => {
    writeText(stream, 200, svgMarkup(asset.title, asset.background, asset.accent), 'image/svg+xml; charset=utf-8');
  });
  return true;
}

function routeSignalboard(req, res) {
  const url = new URL(req.url, 'http://127.0.0.1');
  const { pathname } = url;

  if (pathname === '/signalboard' || pathname === '/signalboard/') {
    writeText(res, 200, INDEX_HTML, 'text/html; charset=utf-8');
    return;
  }
  if (pathname === '/signalboard/app.css') {
    writeText(res, 200, APP_CSS, 'text/css; charset=utf-8');
    return;
  }
  if (pathname.startsWith('/signalboard/api/')) {
    if (handleApi(pathname, url, res) === false) {
      writeText(res, 404, 'not found', 'text/plain; charset=utf-8');
    }
    return;
  }
  if (pathname.startsWith('/signalboard/assets/')) {
    handleAsset(pathname, res);
    return;
  }

  writeText(res, 404, 'not found', 'text/plain; charset=utf-8');
}

async function startSignalboardServer() {
  const server = http.createServer(routeSignalboard);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  return {
    url() {
      return `http://127.0.0.1:${address.port}/signalboard/`;
    },
    async close() {
      await new Promise((resolve, reject) => {
        server.close((error) => {
          if (error) {
            reject(error);
            return;
          }
          resolve();
        });
      });
    },
  };
}

module.exports = {
  startSignalboardServer,
};
