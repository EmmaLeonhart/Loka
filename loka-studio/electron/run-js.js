// Launch the Electron shell pointed at the JS Studio (web-studio/)
// instead of the frozen Flutter web build.
//
// Sets STUDIO_WEB_ROOT (so server.js serves ../../web-studio) and a
// distinct STUDIO_WEB_PORT (8091) so it never collides with the
// Flutter build's :8090 server. `npm start` with no env still loads
// the Flutter build — the two paths stay independent.
const path = require('path');
const { spawn } = require('child_process');
const electron = require('electron'); // path to the electron binary

const env = {
  ...process.env,
  STUDIO_WEB_ROOT: path.resolve(__dirname, '..', '..', 'web-studio'),
  STUDIO_WEB_PORT: process.env.STUDIO_WEB_PORT || '8091',
};

const child = spawn(electron, [path.join(__dirname, '.')], {
  stdio: 'inherit',
  env,
});
child.on('close', (code) => process.exit(code ?? 0));
