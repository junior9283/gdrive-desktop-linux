import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createApi } from '../api.js';

// O que o Tauri injeta na página (`__TAURI_INTERNALS__`), com o serviço fora do ar.
function fakeTauri(serviceDown = true) {
  const callbacks = new Map();
  const calls = [];
  let next = 1;
  const tauri = {
    calls,
    serviceDown,
    fire(event, payload) {
      for (const { event: name, handler } of calls.filter((c) => c.cmd === 'plugin:event|listen').map((c) => c.args)) {
        if (name === event) callbacks.get(handler)({ event, payload });
      }
    },
    transformCallback(fn) {
      callbacks.set(next, fn);
      return next++;
    },
    async invoke(cmd, args) {
      calls.push({ cmd, args });
      if (cmd === 'call' && tauri.serviceDown) throw { name: 'Unavailable', message: 'ServiceUnknown' };
      return null;
    },
  };
  return tauri;
}

test('chamada sem serviço avisa que ele caiu e repassa o erro', async () => {
  const tauri = fakeTauri();
  const api = createApi(() => tauri);
  const changes = [];
  api.onService((up) => changes.push(up));

  await assert.rejects(api.call('status'), { name: 'Unavailable' });

  assert.deepEqual(tauri.calls.at(-1), { cmd: 'call', args: { method: 'status', args: {} } });
  assert.deepEqual(changes, [false]);
  assert.equal(api.serviceUp, false);
});

test('service-up avisa que o serviço voltou', async () => {
  const tauri = fakeTauri();
  const api = createApi(() => tauri);
  const changes = [];
  await api.onService((up) => changes.push(up));
  await assert.rejects(api.call('status'));

  tauri.fire('service-up', null);

  assert.deepEqual(changes, [false, true]);
  assert.equal(api.serviceUp, true);
});

test('uma chamada que dá certo mostra que o serviço voltou, mesmo sem o service-up', async () => {
  const tauri = fakeTauri();
  const api = createApi(() => tauri);
  const changes = [];
  await api.onService((up) => changes.push(up));
  await assert.rejects(api.call('status'));

  // O serviço reiniciou e uma atualização da tela chegou antes do tique da ponte.
  tauri.serviceDown = false;
  await api.call('status');

  assert.deepEqual(changes, [false, true]);
  assert.equal(api.serviceUp, true);
});

test('service-down avisa que o serviço caiu sem esperar uma chamada falhar', async () => {
  const tauri = fakeTauri(false);
  const api = createApi(() => tauri);
  const changes = [];
  await api.onService((up) => changes.push(up));

  tauri.fire('service-down', null);

  assert.deepEqual(changes, [false]);
  assert.equal(api.serviceUp, false);
});

test('eventos chegam com o conteúdo', async () => {
  const tauri = fakeTauri();
  const api = createApi(() => tauri);
  const got = [];
  await api.on('login-finished', (payload) => got.push(payload));

  tauri.fire('login-finished', { ok: true, email: 'a@b.c', error: '' });

  assert.deepEqual(got, [{ ok: true, email: 'a@b.c', error: '' }]);
});
