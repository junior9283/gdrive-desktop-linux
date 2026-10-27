import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mirrorConfirm, suggestedMirror } from '../screens/preferences/mirror.js';
import { refusalKey } from '../screens/preferences/folders.js';

test('a pasta sugerida para o Espelho fica ao lado da montagem', () => {
  assert.equal(suggestedMirror('/home/ana/Google Drive', 'Meu Drive'), '/home/ana/Meu Drive');
  assert.equal(suggestedMirror('/home/ana/Drive/', 'Meu Drive'), '/home/ana/Meu Drive');
  assert.equal(suggestedMirror('', 'Meu Drive'), '');
});

test('a confirmação mostra o tamanho, o espaço e se cabe', () => {
  assert.deepEqual(mirrorConfirm({ refusal: '', size: 10, free: 100, pending: 0 }),
    { refusal: null, size: 10, free: 100, fits: true, pending: 0, ok: true });
  assert.equal(mirrorConfirm({ refusal: '', size: 200, free: 100, pending: 0 }).ok, false);
  assert.equal(mirrorConfirm({ refusal: '', size: 1, free: 2, pending: 3 }).pending, 3);
});

test('as recusas do Espelho têm texto próprio', () => {
  assert.deepEqual(mirrorConfirm({ refusal: 'nested', size: 0, free: 0, pending: 0 }), { refusal: 'folder-refused-nested', ok: false });
  assert.equal(mirrorConfirm({ refusal: 'mirror-disabled', size: 0, free: 0, pending: 0 }).refusal,
    'folder-refused-mirror-disabled');
  assert.equal(mirrorConfirm({ refusal: 'estranho', size: 0, free: 0, pending: 0 }).refusal, 'folder-refused-unreadable');
  assert.equal(refusalKey('mirror-pending'), 'folder-refused-mirror-pending');
});
