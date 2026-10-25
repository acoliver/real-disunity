import assert from 'node:assert/strict';
import {test} from 'node:test';
import {deflateSync} from 'node:zlib';
import {pngEvidence} from './browser-evidence.mjs';

test('production PNG evidence reads the flattened serialized capture path and validates bytes', async () => {
  const chunk=(name,data)=>{const b=Buffer.alloc(data.length+12);b.writeUInt32BE(data.length);b.write(name,4);data.copy(b,8);return b;};
  const header=Buffer.alloc(13);header.writeUInt32BE(960);header.writeUInt32BE(720,4);header[8]=8;header[9]=6;
  const bytes=Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),
    chunk('IDAT',deflateSync(Buffer.alloc(720*(960*4+1)))),chunk('IEND',Buffer.alloc(0))]);
  const evidence=pngEvidence(bytes);
  assert.equal(evidence.width,960);
  assert.equal(evidence.height,720);
  assert(evidence.inflated_bytes>=960*720);
  assert.throws(()=>pngEvidence(bytes.subarray(0,bytes.length-5)),/complete PNG chunk/);
  assert.throws(()=>pngEvidence(Buffer.from('invalid')),/PNG signature/);
});
