import { test } from 'node:test';
import assert from 'node:assert/strict';
import { applied, changed, isOn, parseArg, saveDrives } from '../screens/shared-drives/model.js';

const equipe = { id: 'd1', name: 'Equipe', enabled: true };
const obras = { id: 'd2', name: 'Obras', enabled: false };

test('o argumento traz a conta e, no fim, o pedido de criar um drive', () => {
  assert.deepEqual(parseArg('ana@example.com'), { account: 'ana@example.com', create: false });
  assert.deepEqual(parseArg('ana@example.com/new'), { account: 'ana@example.com', create: true });
  assert.deepEqual(parseArg(null), { account: '', create: false });
  assert.deepEqual(parseArg('/new'), { account: '', create: true });
});

test('cada chave é o que o usuário mexeu ou, sem mexer, o estado do serviço', () => {
  const overrides = { d2: true };
  assert.deepEqual([equipe, obras].map((d) => isOn(d, overrides)), [true, true]);
  // Um drive que acabou de chegar (sem mexida) já cai no estado do serviço.
  assert.equal(isOn({ id: 'd3', enabled: true }, overrides), true);
  assert.equal(isOn({ id: 'd4', enabled: false }, overrides), false);
});

test('só há o que salvar quando alguma chave difere do serviço', () => {
  assert.equal(changed([equipe, obras], {}), false);
  assert.equal(changed([equipe, obras], { d2: true }), true);
  assert.equal(changed([equipe, obras], { d1: false }), true);
  // Ligar e desligar de volta não é mudança.
  assert.equal(changed([equipe, obras], { d1: true, d2: false }), false);
});

test('depois de salvar, o estado do serviço passa a ser o das chaves', () => {
  assert.deepEqual(applied([equipe, obras], { d1: false, d2: true }), [
    { ...equipe, enabled: false },
    { ...obras, enabled: true },
  ]);
});

test('salvar manda a lista toda de drives ligados, na ordem da tela', async () => {
  const calls = [];
  const api = { call: async (method, args) => calls.push([method, args]) };

  const result = await saveDrives(api, 'ana@example.com', [equipe, obras], { d2: true });

  assert.deepEqual(result, { ok: true });
  assert.deepEqual(calls, [['set_shared_drives', { account: 'ana@example.com', enabled: ['d1', 'd2'] }]]);
});

test('o serviço recusa desligar um drive com envio pendente: a tela mostra o motivo', async () => {
  const refused = Object.assign(new Error('Espere os envios'), { name: 'PendingUploads' });
  const api = { call: async () => Promise.reject(refused) };

  const result = await saveDrives(api, '', [equipe], { d1: false });

  assert.deepEqual(result, { ok: false, message: 'Espere os envios' });
});

test('outros erros ao salvar seguem para quem chamou', async () => {
  const broken = Object.assign(new Error('sem rede'), { name: 'Failed' });
  const api = { call: async () => Promise.reject(broken) };

  await assert.rejects(saveDrives(api, '', [equipe], { d1: false }), broken);
});
