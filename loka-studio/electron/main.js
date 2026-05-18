// Electron shell for Loka Studio (web build).
//
// Loads the Flutter-web app served by server.js on :8090. This is the
// "Flutter -> web -> Electron" path: the entire Studio Dart UI,
// unchanged, running in a desktop Electron window instead of the
// Flutter desktop embedder. The app talks to the Loka endpoint
// (default http://localhost:3030) over HTTP, same as the web tab.
const { app, BrowserWindow } = require('electron');
const { spawn } = require('child_process');
const path = require('path');
const http = require('http');

const URL = `http://localhost:${process.env.STUDIO_WEB_PORT || 8090}`;
let staticProc = null;

function waitForServer(url, tries, cb) {
  http.get(url, () => cb()).on('error', () => {
    if (tries <= 0) return cb(); // give up waiting; load anyway
    setTimeout(() => waitForServer(url, tries - 1, cb), 300);
  });
}

function createWindow() {
  const win = new BrowserWindow({
    width: 1480,
    height: 940,
    title: 'Loka Studio',
    backgroundColor: '#07070c',
    autoHideMenuBar: true,
    webPreferences: { contextIsolation: true, nodeIntegration: false },
  });
  win.loadURL(URL);
}

app.whenReady().then(() => {
  // Start the bundled static server unless one is already up.
  staticProc = spawn(process.execPath, [path.join(__dirname, 'server.js')], {
    env: { ...process.env, ELECTRON_RUN_AS_NODE: '1' },
    stdio: 'inherit',
  });
  waitForServer(URL, 40, createWindow);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (staticProc) staticProc.kill();
  if (process.platform !== 'darwin') app.quit();
});
