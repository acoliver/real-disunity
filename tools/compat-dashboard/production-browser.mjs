// Production qualification through installed Chrome and Node's built-in CDP client.
import assert from 'node:assert/strict';
import {spawn, execFileSync} from 'node:child_process';
import {mkdir, mkdtemp, readFile, writeFile, readdir, stat, lstat, rm} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {openSync} from 'node:fs';
import {request as httpRequest} from 'node:http';
import path from 'node:path';
import {pngEvidence, nativeWorkerEvidence} from './browser-evidence.mjs';
import {displayContext} from './display-context.mjs';

const root = process.env.RD_WORKTREE;
assert(root, 'source .session/env.sh first');
const display = await displayContext(root);
const base = path.join(root, '.work/resumed-production-browser');
await mkdir(base, {recursive:true});
const directory = await mkdtemp(path.join(base, 'qualification-'));
for (const name of ['home', 'tmp', 'cache', 'config', 'state', 'chrome', 'runtime']) {
  await mkdir(path.join(directory, name), {mode:0o700});
}
const environment = {...process.env, COMPAT_ROOT:root, HOME:path.join(directory,'home'),
  TMPDIR:path.join(directory,'tmp'), TMP:path.join(directory,'tmp'), TEMP:path.join(directory,'tmp'),
  XDG_CACHE_HOME:path.join(directory,'cache'), XDG_CONFIG_HOME:path.join(directory,'config'),
  XDG_STATE_HOME:path.join(directory,'state'), XDG_RUNTIME_DIR:path.join(directory,'runtime'),
  MESA_SHADER_CACHE_DIR:path.join(directory,'cache/mesa'),
  COMPAT_RUNS:path.join(directory,'runs'), VK_DRIVER_FILES:'/usr/share/vulkan/icd.d/lvp_icd.json', DISPLAY:display.display};
const binary = path.join(process.env.CARGO_TARGET_DIR, 'debug/compat-kit');
const results = {started:new Date().toISOString(), directory, display, checks:{}, findings:[], processes:{}, status:'running',
  scope:{runtime:'frozen legacy provider bundle',strict_admission_qualified:false,connected_refresh_qualified:false,
    offline_refresh:'blocked-result rendering is evaluated separately; connected refresh still requires success'}};
const pending = new Map();
let server, chrome, socket, url, sequence = 0;
const ownedWorkers = new Set();
const browserEvents = [];
const ownedTasks = new Set();
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const json = async file => JSON.parse(await readFile(file, 'utf8'));
async function persist() {
  await writeFile(path.join(directory,'results.json'), JSON.stringify(results,null,2)+'\n');
  await writeFile(path.join(base,'latest.json'), JSON.stringify({directory,status:results.status},null,2)+'\n');
}
async function phase(name, action) {
  results.phase = name;
  const started=new Date().toISOString();
  await persist();
  console.log(JSON.stringify({phase:name,at:new Date().toISOString()}));
  try {
    results.checks[name] = {status:'pass', ...(await action())};
  } catch (error) {
    results.checks[name] = {status:'fail',error:safeDiagnostic(error.stack || error)};
    results.findings.push({phase:name,error:safeDiagnostic(error.message || error)});
  }
  Object.assign(results.checks[name],{started,finished:new Date().toISOString()});
  console.log(JSON.stringify({phase:name,status:results.checks[name].status,at:new Date().toISOString()}));
  await persist();
  return results.checks[name];
}
async function until(check, description, milliseconds=180000, interval=150) {
  const end = Date.now()+milliseconds;
  let last;
  while (Date.now()<end) {
    try {const value = await check(); if(value) return value;} catch(error) {
      if(error.code==='ERR_ASSERTION')throw error;
      last=error;
    }
    await delay(interval);
  }
  throw new Error('Timed out: '+description+(last ? '; '+last.message : ''));
}
async function cdp(method, params={}, timeout=30000) {
  const id = ++sequence;
  const response = new Promise((resolve,reject) => {
    const timer = setTimeout(() => {pending.delete(id);reject(new Error('CDP timeout: '+method));},timeout);
    pending.set(id,{resolve:value=>{clearTimeout(timer);resolve(value);},reject:error=>{clearTimeout(timer);reject(error);}});
  });
  socket.send(JSON.stringify({id,method,params}));
  return response;
}
async function evaluate(expression) {
  const response = await cdp('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});
  if(response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails));
  return response.result.value;
}
async function state() {
  const response = await fetch(url+'api/state', {signal:AbortSignal.timeout(30000)});
  assert.equal(response.status,200);
  return response.json();
}
function safeDiagnostic(value) {
  return String(value).replace(/[a-f0-9]{64}/gi,'[redacted]').slice(0,2048);
}
async function navigationSnapshot(runIds) {
  const snapshot={at:new Date().toISOString(),phase:results.phase,expected_server_run_ids:runIds?.slice(0,100)};
  try {
    snapshot.document=await evaluate(`(()=>{const f=document.querySelector('#fixture');const p=document.querySelector('#profile');let runs;try{runs=JSON.parse(sessionStorage.getItem('compat-runs'));}catch{runs='invalid JSON';}return {previous_document:!!window.__qualificationPreviousDocument,time_origin:performance.timeOrigin,ready:document.readyState,path:location.pathname,selected_fixture:f?.value,selected_profile:p?.value,profiles:[...(p?.options||[])].slice(0,20).map(o=>o.value),stored_run_ids:Array.isArray(runs)?runs.slice(0,100):runs,images:{total:document.images.length,complete:[...document.images].filter(i=>i.complete&&i.naturalWidth>0).length},state_resources:performance.getEntriesByType('resource').filter(r=>new URL(r.name).pathname==='/api/state').slice(-8).map(r=>({duration_ms:r.duration,transfer_bytes:r.transferSize})),action_error:(document.querySelector('#action')?.textContent||'').replace(/[a-f0-9]{64}/gi,'[redacted]').slice(0,2048)};})()`);
  } catch(error) {snapshot.document_error=safeDiagnostic(error.message);}
  (results.navigation_diagnostics??=[]).push(snapshot);
}
async function navigate() {
  const attempt={phase:results.phase,started:new Date().toISOString()};
  (results.navigations??=[]).push(attempt);
  try {
    // No task is started until navigation finishes. Repeated full-state reads compete with image loading.
    attempt.run_ids=(await state()).runs.map(run=>run.run_id);
    const key=JSON.stringify(attempt.run_ids);
    await evaluate('window.__qualificationPreviousDocument=true');
    const response=await cdp('Page.navigate',{url});
    attempt.response={frame_id:response.frameId,loader_id:response.loaderId,error:response.errorText?safeDiagnostic(response.errorText):null};
    await until(()=>evaluate(`!window.__qualificationPreviousDocument && document.readyState==='complete' && !!document.querySelector('#fixture') && document.querySelector('#profile').options.length>1 && sessionStorage.getItem('compat-runs')===${JSON.stringify(key)}`),
      'dashboard ready with current run history',60000);
  } finally {
    await navigationSnapshot(attempt.run_ids);
  }
}
async function select(fixture, profile) {
  await until(()=>evaluate(`!![...document.querySelector('#fixture').options].find(o=>o.value===${JSON.stringify(fixture)})`),'display-name fixture option');
  await evaluate(`(()=>{const f=document.querySelector('#fixture');f.value=${JSON.stringify(fixture)};f.dispatchEvent(new Event('change',{bubbles:true}));})()`);
  await until(()=>evaluate(`(()=>{const f=document.querySelector('#fixture');const p=document.querySelector('#profile');return f.value===${JSON.stringify(fixture)}&&p.options.length>1&&${profile ? `[...p.options].some(o=>o.value===${JSON.stringify(profile)})` : 'true'};})()`),'fixture profile catalog ready',30000);
  const desired=profile || '';
  await evaluate(`(async()=>{const p=document.querySelector('#profile');p.value=${JSON.stringify(desired)};p.dispatchEvent(new Event('change',{bubbles:true}));const d=document.querySelector('#desktop');d.checked=false;d.dispatchEvent(new Event('change',{bubbles:true}));const a=document.querySelector('#accept');a.checked=false;a.dispatchEvent(new Event('change',{bubbles:true}));await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));})()`);
  const selection=await evaluate(`(()=>({fixture:document.querySelector('#fixture').value,profile:document.querySelector('#profile').value,profiles:[...document.querySelector('#profile').options].map(o=>({value:o.value,text:o.textContent})),desktop:document.querySelector('#desktop').checked,accept:document.querySelector('#accept').checked}))()`);
  assert.equal(selection.fixture,fixture);
  if(profile) assert.equal(selection.profile,profile);
  assert.equal(selection.desktop,false);
  assert.equal(selection.accept,false);
  return selection;
}
async function clickAction(text) {
  await evaluate(`document.querySelector('#action').textContent='';[...document.querySelectorAll('#controls button')].find(b=>b.textContent===${JSON.stringify(text)}).click()`);
  return until(async()=>{
    const text = await evaluate("document.querySelector('#action')?.textContent");
    if(!text)return false;
    const value = JSON.parse(text);
    if(value.error)throw new Error(value.error);
    if(value.id)ownedTasks.add(value.id);
    return value.id ? value : false;
  },text+' accepted',30000);
}
async function complete(id, timeout=240000) {
  return until(async()=>{const task=(await state()).tasks.find(t=>t.id===id);return task?.done ? task : false;},'task '+id,timeout,1000);
}
async function processes() {
  const entries = await readdir('/proc');
  const rows = await Promise.all(entries.filter(n=>/^\d+$/.test(n)).map(async name=>{
    try {
      const text = await readFile('/proc/'+name+'/stat','utf8');
      const fields = text.slice(text.lastIndexOf(')')+2).split(' ');
      return {pid:Number(name),ppid:Number(fields[1]),group:Number(fields[2]),state:fields[0],command:(await readFile('/proc/'+name+'/cmdline','utf8')).replaceAll('\0',' ')};
    } catch {return null;}
  }));
  return rows.filter(Boolean);
}
async function sourceSnapshot() {
  const source=path.join(root,'.work/games/snake');
  const run=args=>execFileSync('git',['-C',source,...args],{encoding:'utf8',env:environment}).trim();
  const scripts={};
  async function walk(directory) {
    for(const item of await readdir(directory,{withFileTypes:true})) {
      const file=path.join(directory,item.name);
      if(item.isDirectory() && item.name!=='.git')await walk(file);
      else if(item.isFile()&&item.name.endsWith('.cs'))scripts[path.relative(source,file)]=createHash('sha256').update(await readFile(file)).digest('hex');
    }
  }
  await walk(source);
  let lock=null;
  try {lock=await readFile(path.join(root,'.work/compat/locks/snake.json'),'utf8');}catch(error){if(error.code!=='ENOENT')throw error;}
  return {revision:run(['rev-parse','HEAD']),status:run(['status','--porcelain','--untracked-files=all']),scripts,lock};
}
async function sourceQualification(run,runDirectory) {
  const fixture=results.fixture_inputs['snake-journey'].fixture;
  const request=await json(path.join(runDirectory,'request.json'));
  assert.equal(request.seed,fixture.seed);
  assert.equal(request.timeline.length,fixture.timeline.length);
  assert.deepEqual(request.timeline.map(({frame,phase,action})=>({frame,phase,action})),fixture.timeline.map(({frame,phase,action})=>({frame,phase,action})));
  assert.deepEqual(request.timeline.filter(step=>step.action==='input'),fixture.timeline.filter(step=>step.action==='input'));
  assert.deepEqual(request.assertions,fixture.assertions);
  assert.equal(request.desktop,false);
  assert.equal(request.profile,run.profile);
  const proof=await json(path.join(runDirectory,'build/original-source-count.json'));
  const sourceDirectory=path.join(root,'.work/games/snake');
  assert.equal(proof.source_count,8);
  assert.equal(proof.count,8);
  assert.equal(proof.compiler_inputs_verified,true);
  assert.equal(Object.keys(proof.source_hashes).length,8);
  assert.equal(proof.compiler_inputs.length,8);
  assert.deepEqual([...proof.compiler_inputs].sort(),Object.keys(run.source_before.scripts).sort().map(file=>path.join(sourceDirectory,file)));
  assert(proof.metadata.references.includes('UnityCompat'));
  assert.deepEqual(proof.source_hashes,run.source_before.scripts);
  const assemblyPath=path.resolve(proof.assembly);
  const assemblyRelative=path.relative(runDirectory,assemblyPath);
  assert(assemblyRelative && !assemblyRelative.startsWith('..'+path.sep) && assemblyRelative!=='..' && !path.isAbsolute(assemblyRelative), 'compiled game PE must remain in the run build directory');
  const assembly=await readFile(assemblyPath);
  assert.equal(assembly.toString('ascii',0,2),'MZ','compiled original-source assembly must be a PE file');
  const assemblySha256=createHash('sha256').update(assembly).digest('hex');
  assert.equal(assemblySha256,proof.assembly_sha256);
  return proof;
}
async function runJourney(profile, label) {
  await navigate();
  const selection=await select('SnakeJourney',profile);
  const job=await clickAction('Run');
  const task=await complete(job.id);
  await writeFile(path.join(directory,label+'-task.json'),JSON.stringify(task,null,2)+'\n');
  const run=task.result;
  assert.equal(task.fixture,'SnakeJourney');
  assert.equal(task.profile,profile || 'source-6000.3');
  assert.equal(run.status,'pass',JSON.stringify({messages:run.messages,message:run.message,status:run.status}));
  assert.equal(run.assertions.length,1466);
  assert(run.assertions.every(a=>a.passed));
  assert.equal(run.captures.length,16);
  assert(run.captures.every(c=>c.width===960&&c.height===720));
  const runDirectory=path.join(root,run.run_dir);
  const proof=await sourceQualification(run,runDirectory);
  const pngs=[];
  for(const capture of run.captures) {
    const file=path.join(runDirectory,capture.path);
    const info=pngEvidence(await readFile(file));
    assert.equal(info.sha256,capture.sha256);
    assert.equal(info.width,960);assert.equal(info.height,720);
    pngs.push({path:file,frame:capture.frame,phase:capture.phase,...info});
  }
  await writeFile(path.join(directory,label+'-pngs.json'),JSON.stringify(pngs,null,2)+'\n');
  const identities={};
  for(const item of await readdir(path.join(runDirectory,'build'),{recursive:true})) {
    if(/identity|manifest|native-image/.test(item)) {
      const file=path.join(runDirectory,'build',item);
      if((await stat(file)).isFile())identities[item]=await readFile(file,'utf8');
    }
  }
  await writeFile(path.join(directory,label+'-identities.json'),JSON.stringify(identities,null,2)+'\n');
  assert.deepEqual(run.source_before,run.source_after);
  assert.deepEqual(run.worker.teardown,{completed:true,native_worlds:0,managed_roots:0,pending_readbacks:0});
  const native=await json(path.join(runDirectory,'build/native-image.json'));
  const imagePath=path.resolve(native.image);
  const imageRoot=path.join(root,'.work/compat/native-images');
  const imageRelative=path.relative(imageRoot,imagePath);
  assert(imageRelative && !imageRelative.startsWith('..'+path.sep) && imageRelative!=='..' && !path.isAbsolute(imageRelative), 'native image must remain in process-lifetime worktree storage');
  assert.equal(native.scope,'process');
  assert(Number.isSafeInteger(native.process_id) && native.process_id>0);
  const imageStat=await lstat(imagePath);
  assert(imageStat.isFile() && !imageStat.isSymbolicLink(), 'native image must be a regular file, not a symlink');
  assert.equal(imageStat.size,native.bytes);
  assert.equal(createHash('sha256').update(await readFile(imagePath)).digest('hex'),native.sha256);
  assert.equal(native.sha256,results.runtime.native.sha256);
  assert.equal(createHash('sha256').update(await readFile(results.runtime.player.path)).digest('hex'),results.runtime.player.sha256);
  return {selection,task_id:task.id,run_id:run.run_id,run_dir:run.run_dir,profile:task.profile,assertions:run.assertions.length,captures:run.captures.length,pngs:path.join(directory,label+'-pngs.json'),elapsed_ms:run.elapsed_ms,teardown:run.worker.teardown,source_qualification:{assembly:proof.assembly,source_count:proof.source_count,compiler_inputs_verified:proof.compiler_inputs_verified}};
}
async function domEvidence(runId,label) {
  await navigate();
  const id=JSON.stringify(runId);
  await until(()=>evaluate(`(()=>{const d=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent.includes(${id}));if(!d)return false;d.open=true;d.scrollIntoView();return d.querySelectorAll('img').length===16 && [...d.querySelectorAll('img')].every(i=>i.complete&&i.naturalWidth===960&&i.naturalHeight===720);})()`),'decoded production captures');
  const evidence=await until(()=>evaluate(`(()=>{const d=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent.includes(${id}));if(!d)return false;d.open=true;const images=[...d.querySelectorAll('img')];if(!images.length||!images.every(i=>i.complete&&i.naturalWidth===960&&i.naturalHeight===720))return false;const pres=d.querySelectorAll('pre');if(pres.length<2||!pres[0].textContent.trim().endsWith('}')||!pres[1].textContent.trim().endsWith('}'))return false;let report,request;try{report=JSON.parse(pres[0].textContent);request=JSON.parse(pres[1].textContent);}catch{return false;}return {summary:d.querySelector('summary').textContent,open:d.open,heading:d.querySelector('h3').textContent,fixture:request.fixture,profile:request.profile,timeline_length:request.timeline.length,inputs:request.timeline.filter(t=>t.action==='input'),expected_observed:report.assertions.slice(0,2),all_passed:report.assertions.every(a=>a.passed),images:images.map(i=>({src:i.src,complete:i.complete,width:i.naturalWidth,height:i.naturalHeight,caption:i.closest('figure').textContent,display:getComputedStyle(i).display,rect:{width:i.getBoundingClientRect().width,height:i.getBoundingClientRect().height}}))};})()`),'complete production details and decoded images');
  assert(evidence.open);
  assert.equal(evidence.fixture,'SnakeJourney');
  const journey=Object.values(results.checks).find(check=>check.run_id===runId&&check.profile);
  assert.equal(evidence.profile,journey.profile);
  assert.equal(evidence.timeline_length,results.fixture_inputs['snake-journey'].fixture.timeline.length);
  assert.match(evidence.heading,/Input timeline/);
  assert(evidence.inputs.length>0);
  assert(evidence.expected_observed.every(a=>a.message.includes('expected')&&a.message.includes('observed')));
  assert.equal(evidence.images.length,16);
  assert(evidence.all_passed);
  assert(evidence.images.every(i=>i.rect.width>0&&i.rect.width<=640&&i.rect.height>0&&i.rect.height<=360));
  await writeFile(path.join(directory,label+'-dom.json'),JSON.stringify(evidence,null,2)+'\n');
  const image=await fetch(evidence.images[0].src);
  assert.equal(image.status,200);
  const bytes=Buffer.from(await image.arrayBuffer());
  const actualPng=pngEvidence(bytes);
  assert.equal(actualPng.width,960);assert.equal(actualPng.height,720);
  await writeFile(path.join(directory,label+'-actual.png'),bytes);
  return {run_id:runId,images:evidence.images.length,width:evidence.images[0].width,height:evidence.images[0].height,timeline_length:evidence.timeline_length,input_count:evidence.inputs.length,details_open:evidence.open,actual_png:actualPng,dom:path.join(directory,label+'-dom.json')};
}
function terminate(child) {if(child)try{process.kill(-child.pid,'SIGKILL');}catch{}}
async function cleanupChrome() {
  terminate(chrome);
  const rows=await processes();
  const profile='--user-data-dir='+path.join(directory,'chrome')+' ';
  const database='--database='+path.join(directory,'config')+'/';
  const groups=new Set(rows.filter(row=>row.command.includes(profile)||row.command.includes(database)).map(row=>row.group));
  if(chrome)groups.add(chrome.pid);
  for(const group of groups) {
    assert(group>0&&group!==process.pid,'refuse to terminate the qualification process group');
    try{process.kill(-group,'SIGKILL');}catch(error){if(error.code!=='ESRCH')throw error;}
  }
  return [...groups];
}
async function filterEvidence() {
  await navigate();
  const baseline=await evaluate(`(()=>({rows:[...document.querySelectorAll('[data-view]')].map((r,index)=>({index,...r.dataset})),statuses:[...document.querySelector('#status-filter').options].map(o=>o.value)}))()`);
  assert.deepEqual(baseline.statuses.filter(Boolean).sort(),['pass','fail','unsupported','not-implemented','blocked','skipped','error','flaky','inconclusive','no record'].sort());
  assert(baseline.rows.some(r=>r.view==='attempt'&&r.status==='skipped'));
  for(const profile of ['source-6000.3','compat-6000.3'])assert(baseline.rows.some(r=>r.view==='attempt'&&r.status==='pass'&&r.profile===profile));
  const selections=[...baseline.statuses.map(status=>({status})),
    {profile:'source-6000.3',status:'pass',attempt:'attempt'},
    {profile:'compat-6000.3',status:'pass',attempt:'attempt'},
    {status:'skipped',attempt:'attempt'},{status:'no record',attempt:'group'}];
  const observations=[];
  for(const selection of selections) {
    await evaluate(`(()=>{const selections=${JSON.stringify(selection)};for(const field of ['case','game','profile','platform','status','attempt']){const select=document.getElementById(field+'-filter');select.value=selections[field]||'';select.dispatchEvent(new Event('change',{bubbles:true}));}})()`);
    const visible=await evaluate(`[...document.querySelectorAll('[data-view]')].map((r,index)=>({index,hidden:r.hidden,display:getComputedStyle(r).display})).filter(r=>!r.hidden&&r.display!=='none').map(r=>r.index)`);
    const expected=baseline.rows.filter(row=>Object.entries(selection).every(([field,value])=>!value||row[field==='attempt'?'view':field]===value)).map(row=>row.index);
    assert.deepEqual(visible,expected,JSON.stringify(selection));
    observations.push({selection,visible});
  }
  await evaluate(`for(const select of document.querySelectorAll('select[id$="-filter"]')){select.value='';select.dispatchEvent(new Event('change',{bubbles:true}));}`);
  return {rows:baseline.rows.length,statuses:baseline.statuses,observations};
}
async function securityPost(token,extra) {
  const body=JSON.stringify({fixture:'__invalid_security_probe__'});
  const headers={'Content-Type':'application/json','Content-Length':Buffer.byteLength(body),'X-Compat-Token':token,...extra};
  return new Promise((resolve,reject)=>{
    const request=httpRequest(new URL('api/run',url),{method:'POST',headers,signal:AbortSignal.timeout(30000)},response=>{
      const chunks=[];
      response.on('data',chunk=>chunks.push(chunk));
      response.on('error',reject);
      response.on('end',()=>{
        try{resolve({status:response.statusCode,body:JSON.parse(Buffer.concat(chunks).toString('utf8'))});}catch(error){reject(error);}
      });
    });
    request.on('error',reject);
    request.end(body);
  });
}
async function securityEvidence() {
  const before=await state();
  const token=await evaluate(`document.querySelector('meta[name="compat-token"]').content`);
  assert.match(token,/^[a-f0-9]{64}$/);
  const observations=[];
  for(const route of ['run','refresh','stop']) {
    for(const credential of [null,'invalid']) {
      const observation=await evaluate(`(async()=>{const headers={'Content-Type':'application/json'};const credential=${JSON.stringify(credential)};if(credential)headers['X-Compat-Token']=credential;const response=await fetch('/api/${route}',{method:'POST',headers,body:JSON.stringify({fixture:'SnakeJourney',id:'unowned-task'})});return {status:response.status,body:await response.json()};})()`);
      assert.equal(observation.status,403);
      assert.equal(observation.body.error,'launch token required');
      observations.push({route,credential:credential?'invalid':'missing',...observation});
    }
  }
  for(const [name,extra] of [['Origin',{Origin:'http://rejected.invalid'}],['Host',{Host:'rejected.invalid'}]]) {
    const observation=await securityPost(token,extra);
    assert.equal(observation.status,403);
    assert.equal(observation.body.error,'Host or Origin rejected');
    observations.push({rejected:name,...observation});
  }
  const response=await fetch(url,{signal:AbortSignal.timeout(30000)});
  assert.equal(response.status,200);
  const headers=Object.fromEntries(['content-security-policy','x-frame-options','x-content-type-options','cache-control','referrer-policy'].map(name=>[name,response.headers.get(name)]));
  await response.arrayBuffer();
  assert.match(headers['content-security-policy'],/connect-src 'self'/);
  assert.match(headers['content-security-policy'],/frame-ancestors 'none'/);
  assert.equal(headers['x-frame-options'],'DENY');
  assert.equal(headers['x-content-type-options'],'nosniff');
  assert.equal(headers['cache-control'],'no-store');
  assert.equal(headers['referrer-policy'],'no-referrer');
  const after=await state();
  assert.deepEqual(after.tasks,before.tasks,'rejected requests changed tasks');
  assert.deepEqual(after.runs.map(r=>r.run_id),before.runs.map(r=>r.run_id),'rejected requests changed history');
  return {observations,headers,no_task_or_history_mutation:true,bootstrap_token_retained:false};
}
async function offlineRefreshEvidence() {
  const task=await json(path.join(directory,'refresh-task.json'));
  const result=task.result;
  assert.equal(result.status,'blocked');
  assert.equal(result.accepted,false);
  assert.equal(result.failure_cause.kind,'infrastructure');
  assert.equal(result.failure_cause.stage,'refresh-configuration');
  assert.match(result.message,/source update failed: exit status: 128/);
  assert.equal(result.integrity.engine_proof,false);
  assert.equal(result.integrity.scope,'harness-gates-only');
  const rendered=await until(()=>evaluate(`(()=>{const p=[...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(task.id)}));if(!p)return false;const start=p.textContent.indexOf('{');if(start<0)return false;return {result:JSON.parse(p.textContent.slice(start)),stop_present:!!p.querySelector('button')};})()`),'blocked refresh result in production DOM',30000);
  assert.deepEqual(rendered.result,result);
  assert.equal(rendered.stop_present,false);
  const preservation=await json(path.join(directory,'refresh-source-preservation.json'));
  assert.deepEqual(preservation.after,preservation.before);
  const git=[];
  for(const name of await readdir(result.output)) {
    if(name.startsWith('git-'))git.push({path:path.join(result.output,name,'stderr.log'),stderr:await readFile(path.join(result.output,name,'stderr.log'),'utf8')});
  }
  assert(git.some(log=>log.stderr.includes('Could not resolve host: github.com')),'offline disposition requires the actual remote DNS failure');
  return {task_id:task.id,result,rendered,git,source_and_lock_preserved:true,scope:'offline blocked-result rendering only',connected_refresh_qualified:false};
}
const watchdog=setTimeout(()=>{results.findings.push({phase:results.phase,error:'1700 second phase watchdog'});persist().finally(()=>{terminate(chrome);terminate(server);process.exit(2);});},1700000);
try {
  await phase('browser-start',async()=>{
    results.binary={path:binary,modified:(await stat(binary)).mtime.toISOString(),sha256:createHash('sha256').update(await readFile(binary)).digest('hex')};
    results.runtime={};
    for(const [name,file] of [['player','compat-player'],['native','libworldrunner_disunity_ffi.so']]) {
      const full=path.join(process.env.CARGO_TARGET_DIR,'debug',file);
      results.runtime[name]={path:full,sha256:createHash('sha256').update(await readFile(full)).digest('hex')};
    }
    results.environment={DISPLAY:environment.DISPLAY,VK_DRIVER_FILES:environment.VK_DRIVER_FILES,COMPAT_RUNS:environment.COMPAT_RUNS};
    results.original_before=await sourceSnapshot();
    results.fixture_inputs={};
    for(const name of ['snake','snake-journey']) {const file=path.join(root,'fixtures',name+'.json');const bytes=await readFile(file);results.fixture_inputs[name]={sha256:createHash('sha256').update(bytes).digest('hex'),fixture:JSON.parse(bytes)};}
    server=spawn(binary,['serve','--port','0'],{cwd:root,env:environment,detached:true,stdio:['ignore','pipe','pipe']});
    results.processes.server=server.pid;
    let stdout='';
    server.stdout.on('data',data=>{stdout+=data;writeFile(path.join(directory,'server-stdout.log'),stdout).catch(()=>{});});
    let stderr='';
    server.stderr.on('data',data=>{stderr+=data;writeFile(path.join(directory,'server-stderr.log'),stderr).catch(()=>{});});
    url=await until(()=>stdout.includes('\n')&&JSON.parse(stdout.split('\n')[0]).url,'localhost port-zero server',30000);
    const profile=path.join(directory,'chrome');
    // The owned absolute TMPDIR exceeds Linux's Unix-socket path limit in Chrome.
    const temporaryAlias='/proc/'+process.pid+'/fd/'+openSync(path.join(directory,'tmp'),'r');
    const chromeEnvironment={...environment,TMPDIR:temporaryAlias,TMP:temporaryAlias,TEMP:temporaryAlias};
    chrome=spawn('/usr/bin/google-chrome',['--no-sandbox','--disable-gpu','--disable-features=Vulkan','--no-first-run','--no-default-browser-check','--disable-background-networking','--disable-crash-reporter','--disable-dev-shm-usage','--remote-debugging-port=0','--user-data-dir='+profile,'--disk-cache-dir='+path.join(directory,'cache/chrome'),'about:blank'],{cwd:root,env:chromeEnvironment,detached:true,stdio:['ignore',openSync(path.join(directory,'chrome-stdout.log'),'a'),openSync(path.join(directory,'chrome-stderr.log'),'a')]});
    results.processes.chrome=chrome.pid;
    const port=await until(async()=>Number((await readFile(path.join(profile,'DevToolsActivePort'),'utf8')).split('\n')[0]),'installed Chrome',30000);
    const tabs=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    socket=new WebSocket(tabs.find(t=>t.type==='page').webSocketDebuggerUrl);
    await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
    socket.addEventListener('message',event=>{const message=JSON.parse(event.data);if(!message.id && /exception|consoleAPICalled|loadingFailed/.test(message.method||''))browserEvents.push({at:new Date().toISOString(),...message});const promise=pending.get(message.id);if(promise){pending.delete(message.id);if(message.error)promise.reject(new Error(JSON.stringify(message.error)));else promise.resolve(message.result);}});
    await cdp('Page.enable');
    await cdp('Runtime.enable');
    await cdp('Network.enable');
    await cdp('Emulation.setDeviceMetricsOverride',{width:1280,height:1000,deviceScaleFactor:1,mobile:false});
    await navigate();
    return {url,browser:await cdp('Browser.getVersion'),catalog:(await state()).fixtures.map(f=>({name:f.name,profiles:f.profiles}))};
  });
  assert.equal(results.checks['browser-start'].status,'pass','browser startup is required');
  await phase('source-journey',()=>runJourney(null,'source'));
  await phase('compat-journey',()=>runJourney('compat-6000.3','compat'));
  for(const label of ['source','compat']) {
    const check=results.checks[label+'-journey'];
    if(check.status==='pass')await phase(label+'-image-details',()=>domEvidence(check.run_id,label));
  }
  await phase('stop-real-worker',async()=>{
    await navigate();
    await select('SnakeJourney','compat-6000.3');
    const job=await clickAction('Run');
    const worker=await until(async()=>{
      const worker=(await processes()).find(p=>p.ppid===server.pid&&p.command.includes('/compat-player '));
      if(!worker)return false;
      const maps=await readFile('/proc/'+worker.pid+'/maps','utf8');
      const mappings=nativeWorkerEvidence(maps);
      if(!mappings)return false;
      const environ=(await readFile('/proc/'+worker.pid+'/environ','utf8')).split('\0');
      const request=environ.find(value=>value.startsWith('COMPAT_REQUEST='))?.slice('COMPAT_REQUEST='.length);
      if(!request?.startsWith(path.join(directory,'runs')+'/'))return false;
      return {...worker,mappings,request};
    },'active real CoreCLR/native game worker',120000,100);
    assert.notEqual(worker.group,server.pid,'worker must have its own group');
    ownedWorkers.add(worker.group);
    const active=(await processes()).filter(p=>p.group===worker.group);
    const unrelatedBefore=(await processes()).filter(p=>p.pid!==server.pid&&p.ppid!==server.pid&&p.command.includes('/compat-player '));
    await writeFile(path.join(directory,'stop-worker-evidence.json'),JSON.stringify({worker,unrelatedBefore},null,2)+'\n');
    await writeFile(path.join(directory,'stop-active-processes.json'),JSON.stringify(active,null,2)+'\n');
    await until(()=>evaluate(`!![...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(job.id)})&&p.querySelector('button'))`),'task-specific browser Stop');
    await evaluate(`[...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(job.id)})).querySelector('button').click()`);
    const task=await complete(job.id,120000);
    await writeFile(path.join(directory,'stop-task.json'),JSON.stringify(task,null,2)+'\n');
    assert.equal(task.result.status,'skipped',JSON.stringify(task.result.messages));
    const remaining=await until(async()=>{const group=(await processes()).filter(p=>p.group===worker.group&&p.state!=='Z');return group.length===0 ? {processes:group} : false;},'worker process-group cleanup',30000);
    await until(()=>evaluate(`![...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(job.id)}))?.querySelector('button')`),'Stop control removed');
    const visible=await until(()=>evaluate(`(()=>{const p=[...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(job.id)}));return p?.textContent.includes('"status":"skipped"')&&!p.querySelector('button')?p.textContent:false;})()`),'cancelled result rendered without Stop');
    return {task_id:task.id,run_id:task.result.run_id,result:task.result.status,messages:task.result.messages,worker,active,remaining:remaining.processes,coreclr_and_native_loaded:true,visible};
  });
  for(const label of ['source','compat']) {
    const check=results.checks[label+'-journey'];
    if(check.status==='pass')await phase('after-stop-'+label+'-images',()=>domEvidence(check.run_id,'after-stop-'+label));
  }
  await phase('refresh-production-snake',async()=>{
    await navigate();
    const selection=await select('snake');
    assert.equal(selection.accept,false);
    const before=await sourceSnapshot();
    const job=await clickAction('Refresh source');
    const task=await complete(job.id,600000);
    const after=await sourceSnapshot();
    await writeFile(path.join(directory,'refresh-task.json'),JSON.stringify(task,null,2)+'\n');
    await writeFile(path.join(directory,'refresh-source-preservation.json'),JSON.stringify({before,after},null,2)+'\n');
    assert.deepEqual(after,before,'original source or active lock changed');
    assert.equal(task.result.accepted,false);
    const visible=await until(()=>evaluate(`(()=>{const p=[...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(job.id)}));return p?.textContent.includes('"status"')?p.textContent:false;})()`),'visible refresh result',30000);
    await writeFile(path.join(directory,'refresh-visible.txt'),visible+'\n');
    assert.equal(task.result.status,'pass',JSON.stringify(task.result));
    assert.equal(task.result.gates.length,2);
    assert(task.result.gates.every(g=>g.exit==='exit status: 0'&&!g.interrupted));
    const proof=await json(path.join(task.result.output,'candidate-assembly.json'));
    assert.equal(proof.source_count,Object.keys(proof.source_hashes).length);
    const fixture=await json(path.join(task.result.output,'candidate-fixture.json'));
    assert.equal(fixture.metadata.assembly_qualification,path.join(task.result.output,'candidate-assembly.json'));
    assert.equal(fixture.metadata.script_count,proof.source_count);
    return {task_id:task.id,result:task.result,source_preserved:true,accept:false,qualification_source_count:proof.source_count,visible:true};
  });
  if(results.checks['refresh-production-snake'].status==='fail')await phase('offline-refresh-blocked-rendering',offlineRefreshEvidence);
  await phase('result-filters',filterEvidence);
  await phase('security',securityEvidence);
  const compat=results.checks['compat-journey'];
  if(compat.status==='pass')await phase('compositor-screenshot',async()=>{
    await domEvidence(compat.run_id,'final');
    await evaluate(`(()=>{const d=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent.includes(${JSON.stringify(compat.run_id)}));d.querySelector('img').scrollIntoView();})()`);
    const shot=await cdp('Page.captureScreenshot',{format:'png',captureBeyondViewport:false,fromSurface:true},10000);
    const file=path.join(directory,'dashboard-compositor.png');
    await writeFile(file,Buffer.from(shot.data,'base64'));
    return {path:file,...pngEvidence(Buffer.from(shot.data,'base64'))};
  });
} catch(error) {
  results.findings.push({phase:results.phase,error:safeDiagnostic(error.stack||error)});
} finally {
  clearTimeout(watchdog);
  for(const id of ownedTasks) {try {const task=(await state()).tasks.find(t=>t.id===id);if(task&&!task.done)await evaluate(`(()=>{const p=[...document.querySelectorAll('#tasks p')].find(p=>p.textContent.includes(${JSON.stringify(id)}));p?.querySelector('button')?.click();})()`);}catch{}}
  await delay(500);
  for(const row of await processes())if(row.ppid===server?.pid&&row.command.includes('/compat-player '))ownedWorkers.add(row.group);
  for(const group of ownedWorkers)try{process.kill(-group,'SIGKILL');}catch{}
  try{results.original_after=await sourceSnapshot();assert.deepEqual(results.original_after,results.original_before);}catch(error){results.findings.push({phase:'source-preservation',error:String(error)});}
  await writeFile(path.join(directory,'browser-events.json'),JSON.stringify(browserEvents,null,2).replace(/[a-f0-9]{64}/gi,'[redacted]')+'\n');
  socket?.close();
  results.processes.chrome_groups=await cleanupChrome();
  terminate(server);
  await delay(300);
  const rows=await processes();
  results.cleanup={owned_server_alive:rows.some(p=>p.pid===server?.pid&&p.state!=='Z'),owned_chrome_group_alive:rows.some(p=>p.group===chrome?.pid&&p.state!=='Z'),owned_chrome_helpers_alive:rows.filter(p=>results.processes.chrome_groups.includes(p.group)&&p.state!=='Z'),owned_worker_groups_alive:rows.filter(p=>ownedWorkers.has(p.group)&&p.state!=='Z'),xvfb97:rows.filter(p=>p.command.includes('Xvfb :97')).map(p=>p.pid)};
  if(results.cleanup.owned_server_alive||results.cleanup.owned_chrome_group_alive||results.cleanup.owned_chrome_helpers_alive.length||results.cleanup.owned_worker_groups_alive.length) {
    results.findings.push({phase:'cleanup',error:'owned browser, server or worker processes survived cleanup'});
  } else {
    const disposable=['home','tmp','cache','config','state','chrome','runtime'];
    for(const name of disposable)await rm(path.join(directory,name),{recursive:true,force:true});
    results.cleanup.removed_disposable_directories=disposable;
  }
  results.finished=new Date().toISOString();
  results.status=Object.values(results.checks).some(c=>c.status==='fail')||results.findings.length ? 'fail':'pass';
  await persist();
  console.log(JSON.stringify({status:results.status,directory,checks:Object.fromEntries(Object.entries(results.checks).map(([k,v])=>[k,v.status])),cleanup:results.cleanup}));
  process.exitCode=results.status==='pass'?0:1;
}
