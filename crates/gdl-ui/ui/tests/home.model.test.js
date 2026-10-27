import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { I18n, parseFtl } from '../i18n.js';
import { formatSize } from '../format.js';
import { accountMenuFor, headerFor, offlineText, paginate, shownStatus, sortRows } from '../screens/home/model.js';

const load = (lang) =>
  new I18n(
    parseFtl(readFileSync(new URL(`../../../../locales/${lang}/ui.ftl`, import.meta.url), 'utf8')),
    lang,
  );
const pt = load('pt-BR');
const en = load('en');

const now = Date.UTC(2026, 9, 25, 12, 0, 0);
const MIN = 60_000;
const status = (state, extra = {}) => ({
  state,
  syncing: 0,
  pending_uploads: 0,
  paused: state === 'paused',
  offline: state === 'offline',
  last_sync_ms: now - 5 * MIN,
  errors: 0,
  remaining_secs: -1,
  ...extra,
});

function texts(i18n, s) {
  const h = headerFor(s, now, i18n.locale);
  return [i18n.t(...h.title), h.subtitle && i18n.t(...h.subtitle), h.pause && i18n.t(h.pause)];
}

test('cabeçalho para cada estado', () => {
  assert.deepEqual(texts(pt, status('synced')), [
    'Sincronização concluída',
    'Última sincronização há 5 minutos',
    'Pausar',
  ]);
  assert.deepEqual(texts(pt, status('error', { errors: 2 })), [
    'Sincronização incompleta',
    '2 itens não foram sincronizados',
    'Pausar',
  ]);
  assert.deepEqual(texts(pt, status('syncing', { syncing: 3, remaining_secs: 300 })), [
    'Sincronizando 3 itens',
    'Cerca de 5 minutos restantes',
    'Pausar',
  ]);
  assert.deepEqual(texts(pt, status('syncing', { syncing: 1 })), [
    'Sincronizando 1 item',
    'Última sincronização há 5 minutos',
    'Pausar',
  ]);
  assert.deepEqual(texts(pt, status('offline'))[0], 'Sem internet');
  assert.deepEqual(texts(pt, status('quota-full'))[0], 'Armazenamento cheio');
  assert.deepEqual(texts(pt, status('needs-sign-in'))[0], 'Entre de novo na conta');
  assert.deepEqual(texts(pt, status('paused')), [
    'Sincronização pausada',
    'Última sincronização há 5 minutos',
    'Retomar',
  ]);
  assert.deepEqual(texts(pt, status('signed-out')), ['Você não fez login', null, null]);
  assert.deepEqual(texts(en, status('synced')), ['Sync complete', 'Last synced 5 minutes ago', 'Pause']);
});

test('menos de um minuto até 59 s', () => {
  const at = (ms) => texts(pt, status('synced', { last_sync_ms: now - ms }))[1];
  assert.equal(at(0), 'Última sincronização há menos de um minuto');
  assert.equal(at(59_000), 'Última sincronização há menos de um minuto');
  assert.equal(at(60_000), 'Última sincronização há 1 minuto');
  assert.equal(texts(pt, status('synced', { last_sync_ms: 0 }))[1], 'Ainda não houve sincronização');
});

test('paginação', () => {
  const rows = Array.from({ length: 23 }, (_, i) => i);
  const page = paginate(rows, 2, 10);
  assert.deepEqual(page.rows, [10, 11, 12, 13, 14, 15, 16, 17, 18, 19]);
  assert.equal(pt.t('showing', page), 'Mostrando 11–20 de 23');
  assert.equal(page.pages, 3);
  assert.deepEqual(paginate(rows, 3, 10).rows, [20, 21, 22]);
  assert.equal(paginate(rows, 9, 10).page, 3);
  assert.deepEqual(paginate([], 1, 10), { rows: [], page: 1, pages: 1, from: 0, to: 0, total: 0 });
});

test('ordenação estável', () => {
  const rows = [
    { name: 'b', size: 5, time_ms: 1 },
    { name: 'a', size: 1, time_ms: 3 },
    { name: 'c', size: 5, time_ms: 2 },
    { name: 'd', size: 1, time_ms: 4 },
  ];
  const names = (list) => list.map((r) => r.name).join('');
  assert.equal(names(sortRows(rows, 'size', 'asc')), 'adbc');
  assert.equal(names(sortRows(rows, 'size', 'desc')), 'bcad');
  assert.equal(names(sortRows(rows, 'name', 'asc')), 'abcd');
  assert.equal(names(sortRows(rows, 'time', 'desc')), 'dacb');
  assert.equal(names(rows), 'bacd');
});

test('texto do off-line com a contagem e o tamanho', () => {
  assert.deepEqual(offlineText({ pinned_count: 2, pinned_bytes: 1024 }, 'pt-BR'), [
    'card-offline-text',
    { count: 2, size: formatSize(1024, 'pt-BR') },
  ]);
});

const accounts = (...list) => list.map(([email, state = 'synced']) => ({ email, mount_point: `/m/${email}`, state }));

test('menu da conta: as outras contas, sem a mostrada', () => {
  const menu = accountMenuFor({
    account: 'ana@example.com',
    state: 'synced',
    accounts: accounts(['ana@example.com'], ['bia@example.com'], ['caio@example.com', 'needs-sign-in']),
    can_add_account: true,
  });
  assert.deepEqual(menu.others, [
    { email: 'bia@example.com', needsSignIn: false },
    { email: 'caio@example.com', needsSignIn: true },
  ]);
  assert.equal(menu.shownNeedsSignIn, false);
  assert.equal(menu.canAdd, true);
});

test('menu da conta: "Adicionar outra conta" some quando o serviço diz que não cabe', () => {
  const menu = accountMenuFor({ account: 'a@x', accounts: accounts(['a@x']), can_add_account: false });
  assert.equal(menu.canAdd, false);
  assert.deepEqual(menu.others, []);
});

test('menu da conta: a mostrada que precisa entrar de novo pede o botão', () => {
  const menu = accountMenuFor({
    account: 'a@x',
    accounts: accounts(['a@x', 'needs-sign-in'], ['b@x']),
    can_add_account: true,
  });
  assert.equal(menu.shownNeedsSignIn, true);
});

test('cabeçalho da conta que precisa entrar de novo: botão de entrar, sem Pausar', () => {
  const header = headerFor(status('needs-sign-in'), now, 'pt-BR');
  assert.equal(header.signIn, true);
  assert.equal(header.pause, null);
  assert.equal(headerFor(status('synced'), now, 'pt-BR').signIn, false);
});

test('conectando as contas salvas: aviso próprio, sem Pausar nem entrar', () => {
  const header = headerFor(status('starting'), now, 'pt-BR');
  assert.deepEqual(header, { title: ['header-starting', { count: 0 }], subtitle: null, pause: null, signIn: false });
});

test('o cabeçalho mostra o estado da própria conta, não o pior entre elas', () => {
  const both = { account: 'a@x', state: 'error', accounts: accounts(['a@x'], ['b@x', 'error']) };
  assert.equal(shownStatus(both).state, 'synced');
  assert.equal(shownStatus({ account: '', state: 'signed-out', accounts: [] }).state, 'signed-out');
});
