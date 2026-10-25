import { test } from 'node:test';
import assert from 'node:assert/strict';
import { debounce, folderOf, iconFor, moveSelection } from '../screens/search/model.js';

test('debounce chama uma vez só, com o último valor, depois da pausa', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const calls = [];
  const typed = debounce((text) => calls.push(text), 250);

  typed('r');
  t.mock.timers.tick(200);
  typed('re');
  t.mock.timers.tick(249);
  assert.deepEqual(calls, []);
  t.mock.timers.tick(1);

  assert.deepEqual(calls, ['re']);
});

test('debounce cancelado não chama', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const calls = [];
  const typed = debounce((text) => calls.push(text), 250);

  typed('a');
  typed.cancel();
  t.mock.timers.tick(500);

  assert.deepEqual(calls, []);
});

test('ícone pelo tipo do Google e, nos demais arquivos, pela extensão', () => {
  assert.equal(iconFor('application/vnd.google-apps.folder', 'Documentos'), 'folder');
  assert.equal(iconFor('application/vnd.google-apps.document', 'Ata'), 'description');
  assert.equal(iconFor('application/vnd.google-apps.spreadsheet', 'Planilha'), 'table_chart');
  assert.equal(iconFor('application/vnd.google-apps.presentation', 'Aula'), 'slideshow');
  assert.equal(iconFor('application/vnd.google-apps.shortcut', 'Atalho'), 'drive_file_move');
  assert.equal(iconFor('', 'relatório.PDF'), 'picture_as_pdf');
  assert.equal(iconFor('', 'foto.jpg'), 'image');
  assert.equal(iconFor('', 'notas.txt'), 'draft');
  assert.equal(iconFor('', 'sem-extensao'), 'draft');
});

test('setas andam pela lista sem passar das pontas', () => {
  assert.equal(moveSelection(-1, 1, 3), 0);
  assert.equal(moveSelection(0, 1, 3), 1);
  assert.equal(moveSelection(2, 1, 3), 2);
  assert.equal(moveSelection(1, -1, 3), 0);
  assert.equal(moveSelection(0, -1, 3), 0);
  assert.equal(moveSelection(-1, 1, 0), -1);
});

test('o caminho do resultado é a pasta dentro do Drive', () => {
  const mount = '/home/u/Google Drive';

  assert.equal(folderOf(`${mount}/Meu Drive/Documentos/relatório.pdf`, mount), 'Meu Drive/Documentos');
  assert.equal(folderOf(`${mount}/Meu Drive`, mount), '');
  assert.equal(folderOf('/outro/lugar/a.txt', mount), '/outro/lugar');
});
