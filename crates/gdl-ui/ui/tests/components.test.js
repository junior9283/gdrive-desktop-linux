import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { appLogo } from '../components.js';

test('o logotipo das telas é o da variante, e o arquivo existe', () => {
  assert.equal(appLogo(false), 'icons/app.png');
  assert.equal(appLogo(true), 'icons/app-gerasync.png');
  for (const official of [false, true]) {
    assert.ok(existsSync(new URL(`../${appLogo(official)}`, import.meta.url)), appLogo(official));
  }
});
