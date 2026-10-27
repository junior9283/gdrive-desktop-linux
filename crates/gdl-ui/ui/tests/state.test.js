import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createAccountState } from '../state.js';

function fakeWindow() {
  const data = new Map();
  const handlers = [];
  return {
    storage: { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => data.set(k, v) },
    events: { addEventListener: (_, handler) => handlers.push(handler) },
    // Outra janela gravou: só o evento chega aqui.
    otherWindowSets(email) {
      data.set('account', email);
      handlers.forEach((h) => h({ key: 'account' }));
    },
  };
}

test('sem escolha, a conta é a primeira (vazia)', () => {
  const w = fakeWindow();
  assert.equal(createAccountState(() => w.storage, w.events).currentAccount(), '');
});

test('trocar a conta guarda e avisa quem mostra', () => {
  const w = fakeWindow();
  const state = createAccountState(() => w.storage, w.events);
  const seen = [];
  state.onAccountChange((email) => seen.push(email));

  state.setCurrentAccount('ana@example.com');
  state.setCurrentAccount('ana@example.com');

  assert.equal(state.currentAccount(), 'ana@example.com');
  assert.deepEqual(seen, ['ana@example.com']);
});

test('a troca feita em outra janela chega pelo evento storage', () => {
  const w = fakeWindow();
  const state = createAccountState(() => w.storage, w.events);
  const seen = [];
  state.onAccountChange((email) => seen.push(email));

  w.otherWindowSets('bia@example.com');

  assert.deepEqual(seen, ['bia@example.com']);
});
