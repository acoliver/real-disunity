import assert from 'node:assert/strict';
import {test} from 'node:test';
import {validateDisplayContext} from './display-context.mjs';

const context = () => ({
  display: ':103', mount_namespace: 'mnt:[12]', host_mount_namespace: 'mnt:[1]',
  xvfb_mount_namespace: 'mnt:[12]', private_tmp_device_inode: [8, 17],
  host_tmp_device_inode: [8, 1], xvfb_tmp_device_inode: [8, 17],
});
const actual = () => ({display: ':103', namespace: 'mnt:[12]', tmp: [8, 17], backing: [8, 17]});

test('uses the selected contained display without a fixed display number', () => {
  assert.equal(validateDisplayContext(context(), actual()), ':103');
});

test('rejects host namespace and mismatched X server namespace', () => {
  for (const change of [{host_mount_namespace: 'mnt:[12]'}, {xvfb_mount_namespace: 'mnt:[1]'}]) {
    assert.throws(() => validateDisplayContext({...context(), ...change}, actual()), /namespace/);
  }
});

test('rejects display substitution and an unverifiable actual namespace', () => {
  assert.throws(() => validateDisplayContext(context(), {...actual(), display: ':97'}), /display/);
  assert.throws(() => validateDisplayContext(context(), {...actual(), namespace: 'mnt:[13]'}), /namespace/);
});

test('requires the actual tmp directory, backing directory and X server mapping to agree', () => {
  for (const change of [{tmp: [8, 1]}, {backing: [8, 19]}]) {
    assert.throws(() => validateDisplayContext(context(), {...actual(), ...change}), /tmp/);
  }
  assert.throws(() => validateDisplayContext({...context(), xvfb_tmp_device_inode: [8, 19]}, actual()), /tmp/);
  assert.throws(() => validateDisplayContext({...context(), host_tmp_device_inode: [8, 17]}, actual()), /tmp/);
});

test('requires complete current containment evidence', () => {
  for (const property of Object.keys(context())) {
    const evidence = context();
    delete evidence[property];
    assert.throws(() => validateDisplayContext(evidence, actual()));
  }
});
