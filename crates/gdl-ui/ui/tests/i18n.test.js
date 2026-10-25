import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { I18n, parseFtl } from '../i18n.js';

const locale = (lang) => new URL(`../../../../locales/${lang}/ui.ftl`, import.meta.url);

const pt = new I18n(
  {
    hello: 'Olá, { $name }!',
    files: '{ $count ->\n    [0] Nenhum arquivo\n    [one] { $count } arquivo\n   *[other] { $count } arquivos\n}',
    left: 'Faltam { $count ->\n    [one] { $count } item\n   *[other] { $count } itens\n} de { $total }',
  },
  'pt-BR',
);
const en = new I18n(
  { files: '{ $count ->\n    [one] { $count } file\n   *[other] { $count } files\n}' },
  'en',
);

test('interpola variáveis', () => {
  assert.equal(pt.t('hello', { name: 'Ana' }), 'Olá, Ana!');
});

test('plural escolhe one/other pelo idioma', () => {
  assert.equal(pt.t('files', { count: 1 }), '1 arquivo');
  assert.equal(pt.t('files', { count: 3 }), '3 arquivos');
  assert.equal(en.t('files', { count: 1 }), '1 file');
  assert.equal(en.t('files', { count: 0 }), '0 files');
});

test('variante exata vence a categoria', () => {
  assert.equal(pt.t('files', { count: 0 }), 'Nenhum arquivo');
});

test('seleção no meio do texto', () => {
  assert.equal(pt.t('left', { count: 2, total: 5 }), 'Faltam 2 itens de 5');
});

test('chave ausente devolve a chave', () => {
  assert.equal(pt.t('nao-existe'), 'nao-existe');
});

test('variável ausente fica com o nome', () => {
  assert.equal(pt.t('hello'), 'Olá, $name!');
});

test('parseFtl guarda as seleções cruas', () => {
  const map = parseFtl(
    '# comentário\nsimple = Olá, { $name }!\n\nfiles = { $count ->\n    [one] 1 arquivo\n   *[other] { $count } arquivos\n}\n',
  );
  assert.deepEqual(map, {
    simple: 'Olá, { $name }!',
    files: '{ $count ->\n[one] 1 arquivo\n*[other] { $count } arquivos\n}',
  });
});

test('os dois ui.ftl têm as mesmas chaves e formatam', () => {
  const pt = parseFtl(readFileSync(locale('pt-BR'), 'utf8'));
  const en = parseFtl(readFileSync(locale('en'), 'utf8'));
  assert.deepEqual(Object.keys(pt).sort(), Object.keys(en).sort());
  assert.equal(new I18n(pt, 'pt-BR').t('about-version', { version: '1.2' }), 'Versão 1.2');
});
