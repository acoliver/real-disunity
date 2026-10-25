// Harness infrastructure only. Uses the installed Chrome and Node built-ins.
import assert from 'node:assert/strict';
import {openSync, closeSync} from 'node:fs';
import {spawn, execFileSync} from 'node:child_process';
import {mkdtemp, mkdir, readFile, writeFile, rm} from 'node:fs/promises';
import {createInterface} from 'node:readline';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '../../..');
const binaries = path.join(process.env.CARGO_TARGET_DIR || path.join(root, 'target'), 'debug');
const base = path.join(root, '.work/compat-browser-tests');
await mkdir(base, {recursive:true});
const directory = await mkdtemp(path.join(base, 'run-'));
await mkdir(path.join(directory, 'fixtures'));
const fixturePath = path.join(directory, 'fixtures/harness-only.json');
const fixture = {
  schema_version:1, name:'harness-only', game:'snake',
  profiles:['test','unrun','compat-6000.3','compat-2017.1'], default_profile:'test', evidence_label:'harness-only',
  worker:{program:path.join(binaries,'compat-kit-protocol-test-worker'),args:['hang']},
  limits:{deadline_ms:15000,log_bytes:1024,artifact_bytes:1048576,capture_count:4},
  metadata:{reporting:{case_id:'UCK-HAR-001',game:'harness',assertion_dimensions:{'0':'semantic'},units:{'0':'count'}}},
  timeline:[{frame:1,phase:'input',action:'input',keys:['W'],axes:{}}],
  assertions:[{op:'equal',selector:'counter',pointer:'/count',value:7}],
};
await writeFile(fixturePath, JSON.stringify(fixture));
for (const name of ['home','tmp','data','cache','config','state','runtime']) {
  await mkdir(path.join(directory, '.work', name), {recursive:true, mode:0o700});
}
const temporaryFd = openSync(path.join(directory, '.work/tmp'), 'r');
// Keep Chrome's Unix socket paths short while its files remain in the owned directory.
const temporaryAlias = `/proc/${process.pid}/fd/${temporaryFd}`;
const environment = {...process.env,COMPAT_ROOT:directory,COMPAT_RUNS:path.join(directory,'.work/runs'),
  HOME:path.join(directory,'.work/home'),TMPDIR:temporaryAlias,TMP:temporaryAlias,TEMP:temporaryAlias,
  XDG_DATA_HOME:path.join(directory,'.work/data'),XDG_STATE_HOME:path.join(directory,'.work/state'),
  XDG_RUNTIME_DIR:path.join(directory,'.work/runtime'),
  XDG_CACHE_HOME:path.join(directory,'.work/cache'),XDG_CONFIG_HOME:path.join(directory,'.work/config')};
const server = spawn(path.join(binaries,'compat-kit'), ['serve','--port','0'],
  {env:environment,detached:true,stdio:['ignore','pipe','inherit']});
let chrome;
let socket;
const pending = new Map();
let sequence = 0;
async function until(check, message) {
  const deadline = Date.now()+15000;
  while(Date.now()<deadline){try{let value=await check();if(value)return value;}catch{}
    await new Promise(resolve=>setTimeout(resolve,50));}
  throw new Error('Timed out: '+message);
}
async function cdp(method,params={}) {
  const id=++sequence;
  const promise=new Promise((resolve,reject)=>{
    const timeout=setTimeout(()=>{pending.delete(id);reject(new Error('CDP timeout: '+method));},5000);
    pending.set(id,{resolve:value=>{clearTimeout(timeout);resolve(value);},reject:error=>{clearTimeout(timeout);reject(error);}});
  });
  socket.send(JSON.stringify({id,method,params}));
  return promise;
}
async function evaluate(expression) {
  const response=await cdp('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});
  if(response.exceptionDetails)throw new Error(JSON.stringify(response.exceptionDetails));
  return response.result.value;
}
async function terminate(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return;
  const exited = new Promise(resolve => child.once('exit', resolve));
  try { process.kill(-child.pid, 'SIGKILL'); } catch { return; }
  await exited;
}
try {
  const lines=createInterface({input:server.stdout});
  const first=await lines[Symbol.asyncIterator]().next();
  const {url}=JSON.parse(first.value);
  const profile=path.join(directory,'.work/chrome');
  chrome=spawn(process.env.COMPAT_BROWSER || '/usr/bin/google-chrome',['--headless','--no-sandbox','--disable-gpu',
    '--no-first-run','--no-default-browser-check','--disable-background-networking',
    '--disable-crash-reporter','--remote-debugging-port=0','--user-data-dir='+profile,'about:blank'],
    {env:environment,detached:true,stdio:['ignore','ignore','pipe']});
  chrome.stderr.on('data', chunk => process.stderr.write(chunk));
  const port=await until(async()=>Number((await readFile(path.join(profile,'DevToolsActivePort'),'utf8')).split('\n')[0]),'Chrome debugger');
  const tabs=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  socket=new WebSocket(tabs.find(t=>t.type==='page').webSocketDebuggerUrl);
  await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
  socket.addEventListener('message',event=>{const m=JSON.parse(event.data);const p=pending.get(m.id);if(p){pending.delete(m.id);if(m.error)p.reject(m.error);else p.resolve(m.result);}});
  console.error("browser connected",url);
  await cdp('Page.enable');
  await cdp('Page.navigate',{url});
  await until(()=>evaluate("document.body?.textContent.includes('no record')"),'initial dashboard');
  assert(await evaluate("/^[0-9a-f]{64}$/.test(document.querySelector('meta[name=\"compat-token\"]').content)"));
  assert.equal((await fetch(url+'api/run', {method:'POST', headers:{'Content-Type':'application/json'},
    body:JSON.stringify({fixture:'harness-only'})})).status, 403);
  assert.deepEqual((await (await fetch(url+'api/state')).json()).tasks, []);
  await until(()=>evaluate("document.querySelector('#profile').textContent.includes('Unity 6.3') && document.querySelector('#profile').textContent.includes('Legacy Unity 2017.1')"),'explicit version profiles');
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent==='Run').click()");
  await until(()=>evaluate("!!document.querySelector('#tasks button')"),'browser Stop control');
  assert(await evaluate("document.querySelector('#tasks').textContent.includes('harness-only / test')"));
  await evaluate("document.querySelector('#tasks button').click()");
  const state=async()=>await (await fetch(url+'api/state')).json();
  await until(async()=>(await state()).runs.some(r=>r.status==='skipped'),'cancelled real process');
  assert(!await evaluate("document.body.textContent.includes('Refresh source')"));
  fixture.worker.args=['pass'];
  await writeFile(fixturePath,JSON.stringify(fixture));
  await cdp('Page.navigate',{url});
  await until(()=>evaluate("document.readyState==='complete'&&!!document.querySelector('#fixture')"),'updated dashboard');
  await evaluate("[...document.querySelectorAll('button')].find(b=>b.textContent==='Run').click()");
  await until(async()=>(await state()).runs.some(r=>r.status==='pass'),'browser Run completion');
  await cdp('Page.navigate',{url});
  await until(()=>evaluate("[...document.images].some(i=>i.complete&&i.naturalWidth===2)"),'actual validated PNG in browser');
  assert(await evaluate("document.body.textContent.includes('Input timeline') && document.body.textContent.includes('observed')"));
  assert(await evaluate("['pass','fail','unsupported','not-implemented','blocked','skipped','error','flaky','inconclusive','no record'].every(status => [...document.querySelector('#status-filter').options].some(option => option.value === status))"));
  for (const status of ['pass', 'no record']) {
    assert(await evaluate(`(()=>{const filter=document.querySelector('#status-filter');filter.value=${JSON.stringify(status)};filter.dispatchEvent(new Event('change'));const visible=[...document.querySelectorAll('[data-view]')].filter(row=>!row.hidden);return visible.length>0&&visible.every(row=>row.dataset.status===filter.value);})()`));
  }
  for (const [field, value] of [['case','UCK-HAR-001'],['game','harness'],['profile','test'],['platform','linux']]) {
    assert(await evaluate(`(()=>{document.querySelector('#status-filter').value=''; const filter=document.querySelector('#${field}-filter');filter.value=${JSON.stringify(value)};filter.dispatchEvent(new Event('change'));const rows=[...document.querySelectorAll('[data-view]')].filter(r=>!r.hidden);const ok=filter.value===${JSON.stringify(value)}&&rows.length>0&&rows.every(r=>r.dataset.${field}===filter.value);filter.value='';filter.dispatchEvent(new Event('change'));return ok;})()`));
  }
  assert(await evaluate(`(()=>{document.querySelector('#status-filter').value='no record';document.querySelector('#profile-filter').value='unrun';filterResults();const rows=[...document.querySelectorAll('[data-view]')].filter(r=>!r.hidden);return rows.length===1&&rows[0].dataset.profile==='unrun'&&rows[0].dataset.status==='no record';})()`));
  assert(await evaluate(`(()=>{document.querySelector('#status-filter').value='';document.querySelector('#profile-filter').value='';document.querySelector('#attempt-filter').value='attempt';filterResults();const rows=[...document.querySelectorAll('[data-view]')].filter(r=>!r.hidden);return rows.length>=2&&rows.every(r=>r.dataset.view==='attempt');})()`));
  const measured = (await state()).runs.find(r => r.status === 'pass');
  const measuredPath = path.join(directory, measured.run_dir, 'run.json');
  measured.messages.push('</script><img src=x onerror="window.reportInjected=true">');
  await writeFile(measuredPath, JSON.stringify(measured));
  execFileSync(path.join(binaries,'compat-kit'), ['dashboard','.work/static'], {env:environment,stdio:'pipe'});
  await cdp('Page.navigate',{url:'file://'+path.join(directory,'.work/static/index.html')});
  await until(()=>evaluate("document.body?.textContent.includes('Static report.')"),'server-free static report');
  assert(await evaluate("!document.querySelector('meta[name=\"compat-token\"]') && !window.reportInjected && document.body.textContent.includes('</script><img src=x')"));
  assert(await evaluate("(()=>{document.querySelector('#status-filter').value='pass';document.querySelector('#attempt-filter').value='attempt';filterResults();const rows=[...document.querySelectorAll('[data-view]')].filter(r=>!r.hidden);return rows.length===1&&rows[0].dataset.status==='pass'&&rows[0].dataset.view==='attempt';})()"));
  console.error('browser image displayed and status filter verified');
  console.log(JSON.stringify({status:'pass',evidence_label:'harness-only',checks:['missing-token HTTP 403','meta launch token','browser Run','browser Stop','explicit profiles and named tasks','decoded actual image and correlated input/observations','nine outcomes plus no-record filter','case/game/profile/platform/all-attempt filters','combined no-record/profile filter','server-free inert static report'],directory}));
} finally {
  socket?.close();
  await terminate(chrome);
  await terminate(server);
  closeSync(temporaryFd);
  await rm(directory, {recursive:true, force:true});
}
