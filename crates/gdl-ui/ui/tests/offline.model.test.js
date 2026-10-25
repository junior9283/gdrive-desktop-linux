import { test } from 'node:test';
import assert from 'node:assert/strict';
import { segments } from '../screens/offline/model.js';

const GB = 1024 ** 3;
const percents = (summary) => segments(summary).map((s) => s.percent);

test('off-line, cache (sem o que é off-line) e livre', () => {
  const parts = segments({ pinned_bytes: 1 * GB, cache_bytes: 3 * GB, free_bytes: 7 * GB, pinned_count: 1 });

  assert.deepEqual(parts.map((s) => [s.key, s.bytes, s.percent]), [
    ['offline', 1 * GB, 10],
    ['cache', 2 * GB, 20],
    ['free', 7 * GB, 70],
  ]);
});

test('as porcentagens somam 100 mesmo com arredondamento', () => {
  const sum = (summary) => percents(summary).reduce((a, b) => a + b, 0);

  assert.equal(sum({ pinned_bytes: 1, cache_bytes: 2, free_bytes: 1 }), 100);
  assert.equal(sum({ pinned_bytes: 1, cache_bytes: 1, free_bytes: 1 }), 100);
  assert.equal(sum({ pinned_bytes: 7, cache_bytes: 20, free_bytes: 99 }), 100);
  // Um terço para cada: o ponto que sobra vai para o primeiro.
  assert.deepEqual(percents({ pinned_bytes: 1, cache_bytes: 2, free_bytes: 1 }), [34, 33, 33]);
});

test('cache menor que o off-line não fica negativo; tudo zerado é 100% livre', () => {
  assert.deepEqual(percents({ pinned_bytes: 5, cache_bytes: 2, free_bytes: 5 }), [50, 0, 50]);
  assert.deepEqual(percents({ pinned_bytes: 0, cache_bytes: 0, free_bytes: 0 }), [0, 0, 100]);
});
