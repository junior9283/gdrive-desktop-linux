import { test } from 'node:test';
import assert from 'node:assert/strict';
import { currentScreen, itemArg } from '../router.js';

test('a tela vem da query', () => {
  assert.equal(currentScreen('?screen=about'), 'about');
  assert.equal(currentScreen('?screen=notification-settings'), 'notification-settings');
});

test('sem query é a principal', () => {
  assert.equal(currentScreen(''), 'home');
});

test('as telas de item levam a conta e o id', () => {
  assert.deepEqual(itemArg('?screen=share&arg=ana%40example.com%2F1AbC'), { account: 'ana@example.com', id: '1AbC', email: '' });
  assert.deepEqual(itemArg('?screen=versions&arg=1AbC'), { account: '', id: '1AbC', email: '' });
  assert.deepEqual(itemArg('?screen=share'), { account: '', id: null, email: '' });
});

test('Compartilhar pode vir com um e-mail já digitado', () => {
  assert.deepEqual(itemArg('?screen=share&arg=ana%40example.com%2F1AbC%2Fbia%40example.com'), {
    account: 'ana@example.com',
    id: '1AbC',
    email: 'bia@example.com',
  });
});
