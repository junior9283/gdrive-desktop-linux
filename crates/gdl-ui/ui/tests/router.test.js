import { test } from 'node:test';
import assert from 'node:assert/strict';
import { currentScreen } from '../router.js';

test('a tela vem da query', () => {
  assert.equal(currentScreen('?screen=about'), 'about');
  assert.equal(currentScreen('?screen=notification-settings'), 'notification-settings');
});

test('sem query é a principal', () => {
  assert.equal(currentScreen(''), 'home');
});
