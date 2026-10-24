import assert from 'node:assert/strict';
import {lstat, readFile, readlink, realpath, stat} from 'node:fs/promises';
import path from 'node:path';

const identity = value => [value.dev, value.ino];
const same = (left, right) => Array.isArray(left) && Array.isArray(right)
  && left.length === 2 && right.length === 2 && left.every((value, index) => value === right[index]);

export function validateDisplayContext(evidence, actual) {
  assert.match(evidence.display || '', /^:\d+$/, 'contained display is required');
  assert.equal(actual.display, evidence.display, 'selected display differs from containment evidence');
  assert.match(evidence.host_mount_namespace || '', /^mnt:\[\d+\]$/, 'host namespace evidence is required');
  assert.equal(actual.namespace, evidence.mount_namespace, 'current mount namespace differs from evidence');
  assert.equal(evidence.xvfb_mount_namespace, actual.namespace, 'Xvfb namespace differs from the current namespace');
  assert.notEqual(actual.namespace, evidence.host_mount_namespace, 'host mount namespace is not contained');
  assert(same(actual.tmp, evidence.private_tmp_device_inode), 'actual tmp differs from evidence');
  assert(same(actual.backing, actual.tmp), 'worktree tmp backing differs from the actual tmp directory');
  assert(same(evidence.xvfb_tmp_device_inode, actual.tmp), 'Xvfb tmp differs from the actual tmp directory');
  assert(Array.isArray(evidence.host_tmp_device_inode), 'host tmp evidence is required');
  assert(!same(evidence.host_tmp_device_inode, actual.tmp), 'tmp still resolves to host tmp');
  return evidence.display;
}

export async function displayContext(root, environment = process.env) {
  assert(environment.COMPAT_DISPLAY_CONTAINMENT,
    'Run through python3 tools/compat-display/run.py -- node tools/compat-dashboard/production-browser.mjs');
  const work = await realpath(path.join(root, '.work'));
  const file = await realpath(environment.COMPAT_DISPLAY_CONTAINMENT);
  assert(file.startsWith(work + path.sep), 'display containment evidence must be inside the worktree');
  assert((await lstat(environment.COMPAT_DISPLAY_CONTAINMENT)).isFile(), 'display evidence must be a regular file');
  const evidence = JSON.parse(await readFile(file, 'utf8'));
  const actual = {
    display: environment.DISPLAY,
    namespace: await readlink('/proc/self/ns/mnt'),
    tmp: identity(await stat('/tmp')),
    backing: identity(await stat(path.join(path.dirname(file), 'tmp'))),
  };
  return {display: validateDisplayContext(evidence, actual), evidence: file, namespace: actual.namespace};
}
