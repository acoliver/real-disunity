import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import {createServer,request as httpRequest} from 'node:http';
import vm from 'node:vm';

const source=await readFile(new URL('./production-browser.mjs',import.meta.url),'utf8');
const declaration=source.slice(source.indexOf('async function securityPost('),source.indexOf('async function securityEvidence('));

test('security probe transmits the rejected Host instead of substituting the trusted loopback host',async()=>{
  const server=createServer((request,response)=>{
    response.setHeader('Content-Type','application/json');
    response.end(JSON.stringify({host:request.headers.host,origin:request.headers.origin}));
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const url=`http://127.0.0.1:${server.address().port}/`;
  const context=vm.createContext({url,fetch,AbortSignal,httpRequest,Buffer,URL});
  vm.runInContext(declaration,context);
  try {
    const observed=await context.securityPost('test-token',{Host:'rejected.invalid',Origin:'http://rejected.invalid'});
    assert.equal(observed.body.host,'rejected.invalid','the forbidden Host must actually reach the server');
    assert.equal(observed.body.origin,'http://rejected.invalid');
  } finally {
    server.closeAllConnections();
    await new Promise(resolve=>server.close(resolve));
  }
});
