import { remote } from 'webdriverio';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtemp, writeFile, readFile, stat, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import assert from 'node:assert/strict';

const windows = process.platform === 'win32';
const root = await mkdtemp(join(tmpdir(), 'bitburn-e2e-'));
const binary = resolve(process.env.BITBURN_TEST_BINARY ?? `src-tauri/target/debug/BitBurn${windows ? '.exe' : ''}`);
await stat(binary);
const env = { ...process.env, WEBKIT_DISABLE_DMABUF_RENDERER: '1', GDK_BACKEND: 'x11', XDG_CONFIG_HOME: join(root, 'config'), XDG_DATA_HOME: join(root, 'data'), XDG_CACHE_HOME: join(root, 'cache'), WEBVIEW2_USER_DATA_FOLDER: join(root, 'webview') };
const driver = spawn('tauri-driver', ['--port', '4445', '--native-port', '4446'], { env, stdio: ['ignore', 'pipe', 'pipe'] });
let logs = '';
driver.stdout.on('data', data => { logs += data; });
driver.stderr.on('data', data => { logs += data; });
let browser;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  for (let i = 0; ; i++) {
    try { const response = await fetch('http://127.0.0.1:4445/status'); if (response.ok) break; } catch {}
    if (driver.exitCode !== null || i === 100) throw new Error(`Driver did not start: ${logs}`);
    await delay(100);
  }
  const connect = () => remote({ hostname: '127.0.0.1', port: 4445, logLevel: 'error', connectionRetryCount: 0, connectionRetryTimeout: 20000, capabilities: { 'wdio:enforceWebDriverClassic': true, 'tauri:options': { application: binary } } });
  browser = await connect();
  await browser.waitUntil(async () => (await browser.$('h1').getText()) === 'BitBurn', { timeout: 20000 });
  const invoke = (command, args) => browser.executeAsync((command, args, done) => {
    window.__TAURI__.core.invoke(command, args).then(value => done({ value }), error => done({ failure: String(error) }));
  }, command, args);
  const file = join(root, 'selected.txt');
  await writeFile(file, 'Disposable acceptance test data');
  assert.equal((await invoke('get_file_size', { path: file })).value, 31);
  const unconfirmed = await invoke('wipe_files', { paths: [file], passes: 1, algorithm: 'Random' });
  assert.match(unconfirmed.failure, /Confirm this exact wipe request/);
  const rejected = await invoke('wipe_files', { paths: [file], passes: 0, algorithm: 'Random' });
  assert.match(rejected.failure, /between 1 and 35/);
  assert.equal(await readFile(file, 'utf8'), 'Disposable acceptance test data');
  // Real application event path, then real UI and native confirmation.
  await browser.execute(path => window.__TAURI__.event.emit('context_wipe_request', { paths: [path], invalid: [], source: 'context-menu' }), file);
  await browser.$('button=Shred Now').waitForDisplayed({ timeout: 10000 });
  const confirmationClick = browser.$('button=Shred Now').click();
  await delay(100);
  if (!windows) {
    let confirmed = false;
    for (let i = 0; i < 100; i++) {
      try {
        const ids = execFileSync('xdotool', ['search', '--onlyvisible', '--name', 'WARNING'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim().split('\n');
        assert.equal(ids.length, 1, 'Expected one BitBurn confirmation dialog');
        let owner = Number(execFileSync('xdotool', ['getwindowpid', ids[0]], { encoding: 'utf8' }).trim());
        for (let depth = 0; depth < 8 && owner !== driver.pid && owner > 1; depth++) {
          owner = Number(execFileSync('ps', ['-o', 'ppid=', '-p', String(owner)], { encoding: 'utf8' }).trim());
        }
        assert.equal(owner, driver.pid, 'Refusing a dialog outside the test process tree');
        execFileSync('xdotool', ['windowfocus', '--sync', ids[0]]);
        execFileSync('xdotool', ['key', '--clearmodifiers', 'alt+y']);
        confirmed = true;
        break;
      } catch { await delay(100); }
    }
    assert.ok(confirmed, 'Native confirmation dialog was not found');
  } else {
    execFileSync('powershell', ['-NoProfile', '-File', resolve('e2e/confirm.ps1'), '-DriverPid', String(driver.pid)], { stdio: 'inherit' });
  }
  await confirmationClick;
  await browser.waitUntil(async () => (await browser.$('body').getText()).includes('Successfully wiped 1 files'), { timeout: 20000 });
  await assert.rejects(stat(file), { code: 'ENOENT' });
  await browser.execute(() => { localStorage.setItem('theme', 'light'); });
  await browser.deleteSession();
  browser = undefined;
  browser = await connect();
  await browser.waitUntil(async () => (await browser.$('h1').getText()) === 'BitBurn', { timeout: 20000 });
  assert.equal(await browser.$('html').getAttribute('data-theme'), 'light');
  console.log('PASS: boot, IPC validation, real UI wipe, filesystem deletion, persistence and restart');
  await browser.deleteSession();
  browser = undefined;
} catch (error) {
  if (browser) {
    console.error('URL:', await browser.getUrl());
    console.error('Source length:', (await browser.getPageSource()).length);
    console.error('Windows:', await browser.getWindowHandles());
    await writeFile(resolve('e2e-failure.html'), await browser.getPageSource());
  }
  if (!windows) {
    console.error(execFileSync('ps', ['-eo', 'pid,ppid,stat,comm'], { encoding: 'utf8' }));
    try { console.error(execFileSync('xdotool', ['search', '--onlyvisible', '--name', '.*'], { encoding: 'utf8' })); } catch (diagnosticError) { console.error(String(diagnosticError)); }
  }
  console.error(logs);
  throw error;
} finally {
  try { if (browser) await browser.deleteSession(); } finally { driver.kill(); }
  if (driver.exitCode === null && driver.signalCode === null) await new Promise(resolve => driver.once('exit', resolve));
  await rm(root, { recursive: true, force: true });
}
