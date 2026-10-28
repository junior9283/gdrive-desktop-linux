import { test } from 'node:test';
import assert from 'node:assert/strict';
import { groupErrors } from '../screens/errors/model.js';

const item = (name, category) => ({ item_id: name, name, path: `/d/${name}`, category, message: '' });

test('agrupa por categoria, na ordem fixa, sem grupos vazios', () => {
  const errors = [
    item('a', 'lost-and-found'),
    item('b', 'quota'),
    item('c', 'lost-and-found'),
    item('d', 'no-permission'),
  ];

  const groups = groupErrors(errors);

  assert.deepEqual(groups.map((g) => g.category), ['quota', 'no-permission', 'lost-and-found']);
  assert.deepEqual(groups[2].items.map((i) => i.name), ['a', 'c']);
});

test('categoria que a tela não conhece vai para "unknown"', () => {
  const groups = groupErrors([item('x', 'nova-categoria'), item('y', 'unknown')]);

  assert.deepEqual(groups.map((g) => [g.category, g.items.length]), [['unknown', 2]]);
});

test('sem erros, nenhum grupo', () => {
  assert.deepEqual(groupErrors([]), []);
});

test('os limites do Drive têm grupo próprio', () => {
  const errors = ['download-limit', 'folder-too-deep', 'shared-drive-full'].map((category) => ({ category, name: category }));
  assert.deepEqual(groupErrors(errors).map((g) => g.category), ['download-limit', 'folder-too-deep', 'shared-drive-full']);
});
