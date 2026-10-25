import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {createServer} from 'node:http';
import vm from 'node:vm';

const source=await readFile(new URL('./production-browser.mjs',import.meta.url),'utf8');
const declaration=(start,end)=>source.slice(source.indexOf(start),source.indexOf(end));

// Exercise the script's navigation contract without starting its full qualification.
// CDP is represented by DOM states; installed Chrome is qualified separately.
test('waiting for a complete current document does not repeatedly download full run history',async()=>{
  let requests=0;
  const server=createServer((request,response)=>{
    requests++;
    response.setHeader('Content-Type','application/json');
    response.end(JSON.stringify({runs:[{run_id:'current-run'}]}));
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const url=`http://127.0.0.1:${server.address().port}/`;
  const window={};
  let fixture={};
  const profile={options:[{},{}]};
  const document={readyState:'complete',querySelector:key=>key==='#fixture'?fixture:profile};
  let stored='["current-run"]',checks=0;
  const browser=vm.createContext({window,document,sessionStorage:{getItem:()=>stored}});
  const context={
    url,results:{phase:'regression'},safeDiagnostic:String,
    state:async()=>(await fetch(url+'api/state')).json(),
    cdp:async()=>({frameId:'frame',loaderId:'new-document'}),
    navigationSnapshot:async()=>{},
    delay:ms=>new Promise(resolve=>setTimeout(resolve,ms)),
    evaluate:async expression=>{
      if(expression.startsWith('!window.')) {
        checks++;
        window.__previousDashboardDocument=checks===1;
        document.readyState=checks===2?'interactive':'complete';
        profile.options=checks===3?[{}]:[{},{}];
        stored=checks===4?'["stale-run"]':'["current-run"]';
        fixture=checks===5?null:{};
      }
      return vm.runInContext(expression,browser);
    },
  };
  vm.createContext(context);
  vm.runInContext(declaration('async function until(', 'async function cdp(')+
    declaration('async function navigate()', 'async function select('),context);
  try {
    await context.navigate();
    assert.equal(checks,6,'old document, incomplete load, missing profiles, stale history and absent fixture must all wait');
    assert.equal(requests,1,'DOM readiness must use one current history snapshot without a full-state request storm');
  } finally {
    server.closeAllConnections();
    await new Promise(resolve=>server.close(resolve));
  }
});

test('a failed acceptance assertion escapes polling unchanged',async()=>{
  const failure=new assert.AssertionError({message:'production acceptance failed'});
  const context=vm.createContext({delay:async()=>{}});
  vm.runInContext(declaration('async function until(', 'async function cdp('),context);
  let attempts=0;
  await assert.rejects(context.until(async()=>{attempts++;throw failure;},'acceptance',20),error=>error===failure);
  assert.equal(attempts,1,'acceptance failures must not become readiness timeouts');
});
