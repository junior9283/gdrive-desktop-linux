import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createApi } from '../api.js';

// O que o Tauri injeta na página (`__TAURI_INTERNALS__`), com o serviço fora do ar.
function fakeTauri() {
  const callbacks = new Map();
  const calls = [];
  let next = 1;
  return {
    calls,
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
      if (cmd === 'call') throw { name: 'Unavailable', message: 'ServiceUnknown' };
      return null;
    },
  };
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

test('eventos chegam com o conteúdo', async () => {
  const tauri = fakeTauri();
  const api = createApi(() => tauri);
  const got = [];
  await api.on('login-finished', (payload) => got.push(payload));

  tauri.fire('login-finished', { ok: true, email: 'a@b.c', error: '' });

  assert.deepEqual(got, [{ ok: true, email: 'a@b.c', error: '' }]);
});
