import assert from 'node:assert/strict';
import {test} from 'node:test';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {readFile,readdir,mkdtemp,rm} from 'node:fs/promises';
import {setTimeout} from 'node:timers/promises';
import path from 'node:path';
import vm from 'node:vm';

const source=await readFile(new URL('./production-browser.mjs',import.meta.url),'utf8');
const declaration=(start,end)=>source.slice(source.indexOf(start),source.indexOf(end));

test('cleanup removes separately grouped Chrome helpers while preserving another profile',async()=>{
  const base=path.join(process.env.RD_WORKTREE,'.work/resumed-production-browser');
  const directory=await mkdtemp(path.join(base,'cleanup-test-'));
  const launch=argument=>spawn(process.execPath,['-e','setInterval(()=>{},1000)','--',argument],{detached:true,stdio:'ignore'});
  const chrome=launch('--user-data-dir='+path.join(directory,'chrome'));
  const helper=launch('--database='+path.join(directory,'config/google-chrome/Crash Reports'));
  const neighbor=launch('--user-data-dir='+path.join(directory+'-neighbor','chrome'));
  const children=[chrome,helper,neighbor];
  const exits=new Map(children.map(child=>[child,once(child,'exit')]));
  await Promise.all(children.map(child=>once(child,'spawn')));
  const context=vm.createContext({chrome,directory,process,readFile,readdir,path,assert});
  vm.runInContext(declaration('async function processes()', 'async function sourceSnapshot()')+
    declaration('function terminate(', 'async function filterEvidence()'),context);
  try {
    await context.cleanupChrome();
    const done=await Promise.race([exits.get(helper).then(()=>true),setTimeout(2000,false)]);
    assert.equal(done,true,'a separately grouped crash helper survived owned Chrome cleanup');
    await exits.get(chrome);
    assert.doesNotThrow(()=>process.kill(neighbor.pid,0),'another profile must stay alive');
  } finally {
    for(const child of children)try{process.kill(-child.pid,'SIGKILL');}catch{}
    await Promise.all(exits.values());
    await rm(directory,{recursive:true,force:true});
  }
});
