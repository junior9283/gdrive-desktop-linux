import { test } from 'node:test';
import assert from 'node:assert/strict';
import { versionActions, versionsNote } from '../screens/versions/model.js';

const old = { id: 'r1', current: false, keep_forever: false };
const current = { id: 'r2', current: true, keep_forever: false };

test('quem edita baixa, restaura e marca para manter as antigas', () => {
  assert.deepEqual(versionActions(old, true), ['download', 'restore', 'keep']);
  assert.deepEqual(versionActions({ ...old, keep_forever: true }, true), ['download', 'restore', 'unkeep']);
});

test('a atual não se restaura e quem só lê só baixa', () => {
  assert.deepEqual(versionActions(current, true), ['download', 'keep']);
  assert.deepEqual(versionActions(old, false), ['download']);
});

test('só com a versão atual, a tela diz que não há versões anteriores', () => {
  assert.equal(versionsNote([current]), 'versions-none');
  assert.equal(versionsNote([]), 'versions-none');
  assert.equal(versionsNote([current, old]), null);
});
