import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';

const source = await readFile(new URL('../../crates/compat-kit/src/dashboard.js', import.meta.url), 'utf8');
const elements = new Map();
const storage = new Map();
const pending = [];
let interval;
function element(id) {
  if (!elements.has(id)) elements.set(id, {value: '', textContent: '',
    addEventListener() {}, replaceChildren() {}});
  return elements.get(id);
}
const context = vm.createContext({
  document: {getElementById: element, querySelectorAll: () => []},
  location: {protocol: 'http:', reload() {}},
  sessionStorage: {getItem: key => storage.get(key), setItem: (key, value) => storage.set(key, value)},
  setInterval: callback => { interval = callback; },
  fetch: () => new Promise((resolve, reject) => pending.push({resolve, reject})),
});
const flush = async () => { for (let n = 0; n < 12; n++) await Promise.resolve(); };
vm.runInContext(source, context);
assert.equal(pending.length, 1);
for (let n = 0; n < 20; n++) interval();
assert.equal(pending.length, 1, 'slow state response must not create overlapping polls');
pending[0].resolve({ok: true, json: async () => ({fixtures: [], tasks: [], runs: []})});
await flush();
assert.equal(storage.get('compat-runs'), '[]');
interval();
assert.equal(pending.length, 2, 'completed state poll must permit the next interval');
pending[1].reject(new Error('bounded transport failure'));
await flush();
assert.match(element('action').textContent, /bounded transport failure/);
interval();
assert.equal(pending.length, 3, 'failed state poll must release the single-flight guard');
pending[2].resolve({ok: false});
await flush();
interval();
assert.equal(pending.length, 4, 'HTTP rejection must release the single-flight guard');
pending[3].resolve({ok: false});
await flush();
console.log('dashboard state polling: one in flight, completion/error/HTTP retry contracts pass');
