// Minimal static server for the Loka Studio JS app (web-studio/).
//
// Serves the SPA with correct MIME types (incl. `.wasm` as
// application/wasm) and an index.html fallback so deep links / refresh
// work. Same origin for the browser tab and the Electron window (both
// load http://localhost:8090); the app itself talks cross-origin to the
// Loka endpoint (default http://localhost:3030, CORS '*').
const http = require('http');
const fs = require('fs');
const path = require('path');

// STUDIO_WEB_ROOT overrides the served directory; defaults to the
// web-studio/ sibling (the JS Studio — the only Studio since the
// Flutter build was removed 2026-05-30).
const ROOT = process.env.STUDIO_WEB_ROOT
  ? path.resolve(process.env.STUDIO_WEB_ROOT)
  : path.join(__dirname, '..', '..', 'web-studio');
const PORT = process.env.STUDIO_WEB_PORT || 8090;
const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
  '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm',
  '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon', '.ttf': 'font/ttf', '.otf': 'font/otf',
  '.woff': 'font/woff', '.woff2': 'font/woff2', '.map': 'application/json',
};

const server = http.createServer((req, res) => {
  let rel = decodeURIComponent(req.url.split('?')[0]);
  if (rel === '/') rel = '/index.html';
  let file = path.join(ROOT, path.normalize(rel));
  if (!file.startsWith(ROOT)) { res.writeHead(403).end('forbidden'); return; }
  fs.readFile(file, (err, buf) => {
    if (err) {
      // SPA fallback so deep links / refresh work.
      fs.readFile(path.join(ROOT, 'index.html'), (e2, idx) => {
        if (e2) { res.writeHead(404).end('not found'); return; }
        res.writeHead(200, { 'Content-Type': 'text/html' }).end(idx);
      });
      return;
    }
    const mime = MIME[path.extname(file).toLowerCase()] || 'application/octet-stream';
    res.writeHead(200, { 'Content-Type': mime }).end(buf);
  });
});

// If another instance already owns the port (e.g. the standalone
// browser-tab server), don't fight it — that server serves the same
// bundle for the Electron window too. Exit cleanly so Electron's
// waitForServer just connects to the existing one.
server.on('error', (e) => {
  if (e.code === 'EADDRINUSE') {
    console.log(`port ${PORT} already serving — reusing it`);
    process.exit(0);
  }
  throw e;
});
server.listen(PORT, () => {
  console.log(`Loka Studio (web) on http://localhost:${PORT}  (root: ${ROOT})`);
});
