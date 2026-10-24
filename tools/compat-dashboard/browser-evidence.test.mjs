import assert from 'node:assert/strict';
import {test} from 'node:test';
import vm from 'node:vm';
import * as evidence from './browser-evidence.mjs';

function controls(profiles) {
  let value='compat-6000.3';
  const profileControl = {
    options:profiles.map(value=>({value,textContent:value})),
    get value() {return value;},
    set value(next) {value=this.options.some(o=>o.value===next)?next:'';},
  };
  const elements = {
    '#fixture':{value:'breakout', dispatchEvent() {}},
    '#profile':profileControl,
    '#desktop':{checked:true},
    '#accept':{checked:true},
  };
  return {document:{querySelector:key=>elements[key]}, Event:class {}, elements};
}

test('default selection clears the preceding fixture profile so the server uses its default', () => {
  const context=controls(['source-6000.3','compat-6000.3']);
  const selected=vm.runInNewContext(evidence.selectionExpression('SnakeJourney'),context);
  assert.equal(selected.fixture,'SnakeJourney');
  assert.equal(selected.profile,'');
});

test('explicit compatibility selection is retained', () => {
  const context=controls(['source-6000.3','compat-6000.3']);
  assert.equal(vm.runInNewContext(evidence.selectionExpression('SnakeJourney','compat-6000.3'),context).profile,'compat-6000.3');
});

test('selection waits for the production poll to populate profiles', () => {
  const context=controls([]);
  for(const profile of [undefined,'compat-6000.3']) {
    assert.equal(vm.runInNewContext(evidence.selectionExpression('SnakeJourney',profile),context),null);
  }
  context.elements['#profile'].options=[{value:'source-6000.3'},{value:'compat-6000.3'}];
  assert.equal(vm.runInNewContext(evidence.selectionExpression('SnakeJourney','compat-6000.3'),context).profile,'compat-6000.3');
});

test('worker evidence recognizes the actual isolated native library and CoreCLR mappings', () => {
  assert.equal(typeof evidence.nativeWorkerEvidence,'function','native worker mapping evidence is available');
  const maps='7f000000-7f000100 r-xp 00000000 00:01 1 /owned/dotnet/libcoreclr.so\n7f100000-7f100100 r-xp 00000000 00:01 2 /owned/run/build/native-1/libworldrunner_disunity_ffi.so\n';
  const mapped=evidence.nativeWorkerEvidence(maps);
  assert.equal(mapped.coreclr,'/owned/dotnet/libcoreclr.so');
  assert.equal(mapped.native,'/owned/run/build/native-1/libworldrunner_disunity_ffi.so');
  assert.equal(evidence.nativeWorkerEvidence(maps.split('\n')[0]),null);
  assert.equal(evidence.nativeWorkerEvidence(maps.split('\n')[1]),null);
  assert.equal(evidence.nativeWorkerEvidence('unrelated libdisunity_ffi.so text'),null);
});
