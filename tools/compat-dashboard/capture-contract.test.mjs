import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import path from 'node:path';
import {pngEvidence} from './browser-evidence.mjs';

test('production PNG evidence reads the flattened serialized capture path and validates bytes', async () => {
  const root=process.env.RD_WORKTREE;
  assert(root, 'source .session/env.sh');
  const task=JSON.parse(await readFile(path.join(root,'.work/dashboard-production/qualification-Ax8ju0/source-task.json'),'utf8'));
  const capture=task.result.captures[0];
  const bytes=await readFile(path.join(root,task.result.run_dir,capture.path));
  const evidence=pngEvidence(bytes);
  assert.equal(evidence.sha256,capture.sha256);
  assert.equal(evidence.width,960);
  assert.equal(evidence.height,720);
  assert(evidence.inflated_bytes>=960*720);
  assert.throws(()=>pngEvidence(bytes.subarray(0,bytes.length-5)),/complete PNG chunk/);
  assert.throws(()=>pngEvidence(Buffer.from('invalid')),/PNG signature/);
});
