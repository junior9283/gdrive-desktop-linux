import { test } from 'node:test';
import assert from 'node:assert/strict';
import { configurator, folderArg, folderName, folderRows, refusalKey } from '../screens/preferences/folders.js';

const folder = (path, extra = {}) => ({ account: 'ana@example.com', id: 1, path, state: 'active', deletes: 0, ...extra });

test('o nome da pasta é o fim do caminho', () => {
  assert.equal(folderName('/home/ana/Documentos'), 'Documentos');
  assert.equal(folderName('/home/ana/Fotos/'), 'Fotos');
  assert.equal(folderName('/'), '/');
});

test('linhas em ordem de nome, com a conta só quando há mais de uma', () => {
  const rows = folderRows([folder('/home/ana/Imagens', { id: 2 }), folder('/home/ana/Documentos')], false);
  assert.deepEqual(rows.map((r) => [r.name, r.id, r.account, r.accountLabel]), [
    ['Documentos', 1, 'ana@example.com', null],
    ['Imagens', 2, 'ana@example.com', null],
  ]);
  const many = folderRows([folder('/x/Docs')], true);
  assert.equal(many[0].accountLabel, 'ana@example.com');
});

test('o aviso de cada estado; a pasta ativa não tem aviso', () => {
  const notice = (extra) => folderRows([folder('/x/Docs', extra)], false)[0].notice;
  assert.equal(notice({}), null);
  assert.deepEqual(notice({ state: 'stopped-missing' }), ['folder-state-stopped-missing', {}]);
  assert.deepEqual(notice({ state: 'stopped-no-permission' }), ['folder-state-stopped-no-permission', {}]);
  assert.deepEqual(notice({ state: 'confirm-deletes', deletes: 40 }), ['folder-state-confirm-deletes', { count: 40 }]);
});

test('o configurador recusa com texto próprio e pergunta de mesclar só quando já existe', () => {
  assert.deepEqual(configurator({ refusal: 'nested', size: 0, files: 0, exists: false }), {
    refusal: 'folder-refused-nested', size: 0, files: 0, askMerge: false,
  });
  assert.deepEqual(configurator({ refusal: '', size: 2048, files: 3, exists: true }), {
    refusal: null, size: 2048, files: 3, askMerge: true,
  });
  assert.equal(configurator({ refusal: '', size: 0, files: 0, exists: false }).askMerge, false);
});

test('motivos de recusa do serviço viram texto conhecido', () => {
  assert.equal(refusalKey('folders-disabled'), 'folder-refused-folders-disabled');
  assert.equal(refusalKey('network'), 'folder-refused-network');
  assert.equal(refusalKey('outro'), null);
  assert.equal(configurator({ refusal: 'outro', size: 0, files: 0, exists: false }).refusal, 'folder-refused-unreadable');
});

test('o argumento folder: abre o configurador da pasta', () => {
  assert.equal(folderArg('folder:/home/ana/Fotos'), '/home/ana/Fotos');
  assert.equal(folderArg('computer'), null);
  assert.equal(folderArg(null), null);
});

test('só a pasta esperando confirmação pede o botão', () => {
  const rows = folderRows([folder('/x/A', { state: 'confirm-deletes', deletes: 30 }), folder('/x/B', { id: 2 })], false);
  assert.deepEqual(rows.map((r) => r.needsConfirm), [true, false]);
});
