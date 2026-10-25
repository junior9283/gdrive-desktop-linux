import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatSize, formatAgo, formatDuration } from '../format.js';

test('tamanhos em base 1024 com uma casa', () => {
  assert.equal(formatSize(1536, 'pt-BR'), '1,5 KB');
  assert.equal(formatSize(1536, 'en'), '1.5 KB');
  assert.equal(formatSize(0, 'pt-BR'), '0 bytes');
  assert.equal(formatSize(1, 'en'), '1 byte');
  assert.equal(formatSize(1023, 'en'), '1023 bytes');
  assert.equal(formatSize(1024, 'pt-BR'), '1 KB');
  assert.equal(formatSize(5 * 1024 ** 2, 'pt-BR'), '5 MB');
  assert.equal(formatSize(2.25 * 1024 ** 3, 'en'), '2.3 GB');
  assert.equal(formatSize(3 * 1024 ** 4, 'en'), '3 TB');
});

const now = Date.UTC(2026, 9, 25, 12, 0, 0);
const MIN = 60_000;

test('há quanto tempo', () => {
  assert.equal(formatAgo(now - 20_000, now, 'pt-BR'), 'agora');
  assert.equal(formatAgo(now - 5 * MIN, now, 'pt-BR'), 'há 5 minutos');
  assert.equal(formatAgo(now - 5 * MIN, now, 'en'), '5 minutes ago');
  assert.equal(formatAgo(now - 3 * 60 * MIN, now, 'pt-BR'), 'há 3 horas');
  assert.equal(formatAgo(now - 2 * 24 * 60 * MIN, now, 'pt-BR'), 'há 2 dias');
  assert.equal(formatAgo(now - 1 * 24 * 60 * MIN, now, 'en'), '1 day ago');
});

test('tempo restante', () => {
  assert.equal(formatDuration(30, 'pt-BR'), '30 segundos');
  assert.equal(formatDuration(5 * 60, 'pt-BR'), '5 minutos');
  assert.equal(formatDuration(90 * 60, 'en'), '2 hours');
  assert.equal(formatDuration(1, 'en'), '1 second');
});
