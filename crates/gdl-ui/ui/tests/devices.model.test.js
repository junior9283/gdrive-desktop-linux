import { test } from 'node:test';
import assert from 'node:assert/strict';

import { deviceRows } from '../screens/preferences/devices.js';

const device = (over) => ({ id: 'AAAA', label: 'FOTOS', path: '/run/media/u/FOTOS', choice: '', busy: false, ...over });

test('conectado sem escolha: fazer backup e ignorar', () => {
  assert.deepEqual(deviceRows([device()]), [
    { id: 'AAAA', label: 'FOTOS', path: '/run/media/u/FOTOS', choice: '', status: null, actions: ['backup', 'ignore'] },
  ]);
});

test('o estado de cada escolha e o Remover para esquecê-la', () => {
  const [always, ignored, away] = deviceRows([
    device({ id: 'A', label: 'A', choice: 'backup' }),
    device({ id: 'B', label: 'B', choice: 'ignored' }),
    device({ id: 'C', label: 'C', choice: 'ignored', path: '' }),
  ]);
  assert.deepEqual([always.status, always.actions], ['pref-device-always', ['backup', 'ignore', 'forget']]);
  assert.deepEqual([ignored.status, ignored.actions], ['pref-device-ignored', ['backup', 'forget']]);
  assert.deepEqual([away.status, away.actions], ['pref-device-ignored', ['forget']], 'desconectado só sai da lista');
});

test('em andamento não começa outro backup', () => {
  const [row] = deviceRows([device({ busy: true })]);
  assert.deepEqual([row.status, row.actions], ['pref-device-busy', ['ignore']]);
});
