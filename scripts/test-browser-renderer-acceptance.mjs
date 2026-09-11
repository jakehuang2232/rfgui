// Launch an isolated browser profile; never attach to the user's browser session.
// Build artifacts are served only on loopback. Missing WebGPU/software adapters,
// failed assertions, uncaught GPU errors and timeouts are failures, not skips.
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdtemp, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

const directory = resolve(process.argv[2]);
const executable = process.env.CHROME_BIN || (process.platform === 'darwin'
  ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
  : 'google-chrome');
const index = `<!doctype html><meta charset="utf-8"><title>Renderer acceptance</title>
<pre id="status">Starting WebGPU acceptance…</pre><script type="module">
import init, { run_renderer_acceptance } from '/rfgui.js';
const status = document.querySelector('#status');
function fail(error) {
  globalThis.rfguiAcceptanceResult = { status: 'failed', error: String(error), stack: error?.stack, adapter: globalThis.rfguiAcceptanceAdapter,
    progress: globalThis.rfguiAcceptanceProgress, panic: globalThis.rfguiAcceptancePanic };
  status.textContent = JSON.stringify(globalThis.rfguiAcceptanceResult, null, 2);
}
addEventListener('unhandledrejection', e => fail(e.reason));
addEventListener('error', e => fail(e.message));
try {
  if (!navigator.gpu) throw new Error('WebGPU is unavailable');
  const adapter = await navigator.gpu.requestAdapter({ forceFallbackAdapter: false });
  if (!adapter) throw new Error('No WebGPU adapter');
  const info = adapter.info;
  if (!info || info.isFallbackAdapter !== false) throw new Error('Non-fallback WebGPU adapter proof unavailable');
  const witness = { vendor: info.vendor, architecture: info.architecture, device: info.device,
    description: info.description, isFallbackAdapter: info.isFallbackAdapter };
  globalThis.rfguiAcceptanceAdapter = witness;
  globalThis.rfguiAcceptanceProgress = 'Initializing wasm library';
  await init();
  const report = await run_renderer_acceptance();
  if (globalThis.rfguiAcceptanceResult?.status === 'failed') throw new Error('Uncaught error during acceptance');
  globalThis.rfguiAcceptanceResult = { status: 'passed', adapter: witness, report };
  status.textContent = JSON.stringify(globalThis.rfguiAcceptanceResult, null, 2);
} catch (error) { fail(error); }
</script>`;
const server = createServer(async (request, response) => {
  try {
    if (request.url === '/') { response.setHeader('content-type', 'text/html'); response.end(index); return; }
    const files = { '/rfgui.js': 'text/javascript', '/rfgui_bg.wasm': 'application/wasm' };
    if (!files[request.url]) { response.writeHead(404).end(); return; }
    response.setHeader('content-type', files[request.url]);
    response.end(await readFile(join(directory, request.url.slice(1))));
  } catch (error) { response.writeHead(500).end(String(error)); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const profile = await mkdtemp(join(tmpdir(), 'rfgui-browser-acceptance-'));
const browser = spawn(executable, [
  '--headless=new', `--user-data-dir=${profile}`, '--remote-debugging-port=0',
  '--no-first-run', '--no-default-browser-check', '--disable-background-networking',
  '--disable-component-update', 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let launchError;
browser.on('error', error => { launchError = error; });
let stderr = '';
browser.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-1_000_000); });
let socket;
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  let port;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (launchError) throw launchError;
    if (browser.exitCode !== null) throw new Error(`Chrome exited ${browser.exitCode}: ${stderr}`);
    try { port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).trim().split('\n'); break; }
    catch { await pause(100); }
  }
  if (!port) throw new Error('Chrome debugging endpoint did not start');
  socket = new WebSocket(`ws://127.0.0.1:${port[0]}${port[1]}`);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let nextId = 0;
  const pending = new Map();
  socket.onmessage = ({data}) => {
    const message = JSON.parse(data);
    if (!message.id) return;
    const request = pending.get(message.id);
    if (!request) return;
    clearTimeout(request.timer); pending.delete(message.id);
    if (message.error) request.reject(new Error(JSON.stringify(message.error)));
    else request.resolve(message.result);
  };
  const call = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++nextId;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
    pending.set(id, {resolve, reject, timer});
    socket.send(JSON.stringify({id, method, params, ...(sessionId ? {sessionId} : {})}));
  });
  const gpu = await call('SystemInfo.getInfo');
  const version = await call('Browser.getVersion');
  const { targetId } = await call('Target.createTarget', { url: `http://127.0.0.1:${server.address().port}/` });
  const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
  let result, lastProgress;
  for (let attempt = 0; attempt < 600; attempt++) {
    const read = await call('Runtime.evaluate', {
      expression: '({result: globalThis.rfguiAcceptanceResult, progress: globalThis.rfguiAcceptanceProgress, panic: globalThis.rfguiAcceptancePanic})',
      returnByValue: true,
    }, sessionId);
    if (read.exceptionDetails) throw new Error(JSON.stringify(read.exceptionDetails));
    const state = read.result?.value;
    if (state?.progress && state.progress !== lastProgress) {
      lastProgress = state.progress; console.log(lastProgress);
    }
    if (state?.result) { result = state.result; break; }
    if (state?.panic) throw new Error(state.panic);
    await pause(100);
  }
  const report = { browser: version, gpu: gpu.gpu, result: result || {status:'failed', error:'Acceptance timeout', progress:lastProgress} };
  await writeFile(join(directory,'browser-result.json'),JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify(report.result,null,2));
  if (report.result.status !== 'passed') process.exitCode = 1;
  await call('Browser.close').catch(() => {});
} catch (error) {
  process.exitCode = 1;
  const result = {status:'failed',error:String(error)};
  await writeFile(join(directory,'browser-result.json'),JSON.stringify({result},null,2)+'\n');
  console.error(error);
} finally {
  socket?.close();
  await writeFile(join(directory,'chrome.stderr.log'),stderr);
  if (browser.exitCode === null) browser.kill('SIGTERM');
  await Promise.race([new Promise(resolve => browser.once('exit',resolve)),pause(2000)]);
  server.close();
  await rm(profile,{recursive:true,force:true,maxRetries:3,retryDelay:100});
}
