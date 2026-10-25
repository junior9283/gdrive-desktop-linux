// Arquivos off-line: o uso do disco numa barra segmentada (off-line, cache, livre), os itens
// off-line pasta a pasta (com o caminho de volta no alto), "Remover acesso off-line" por item e
// as ações de cache. A lógica da barra fica em `offline/model.js`.

import { attempt, confirmDialog, el, icon, stylesheet, toast } from '../components.js';
import { formatSize } from '../format.js';
import { segments } from './offline/model.js';

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta): a
// pasta aberta continua.
const screen = { ctx: null, usage: null, crumbs: null, items: null, trail: [] };

export async function render(root, ctx) {
  const css = stylesheet('screens/offline.css');
  screen.ctx = ctx;
  const { api, t } = ctx;
  screen.usage = el('section', { class: 'usage', 'aria-label': t('offline-usage') });
  screen.crumbs = el('nav', { class: 'crumbs', 'aria-label': t('offline-title') });
  screen.items = el('div', { class: 'items' });
  const loaded = load();
  await css;
  root.replaceChildren(
    el(
      'div',
      { class: 'offline' },
      el('h1', {}, t('offline-title')),
      screen.usage,
      screen.crumbs,
      screen.items,
      el(
        'div',
        { class: 'actions' },
        el('button', { class: 'text cache-settings', onclick: () => api.openScreen('preferences', 'settings') }, t('offline-cache-settings')),
        el('button', { class: 'tonal clear-cache', onclick: clearCache }, t('offline-clear-cache')),
        el('button', { class: 'tonal clear-offline', onclick: clearOffline }, t('offline-clear-offline')),
        el('button', { class: 'filled refresh', onclick: () => attempt(load()) }, t('offline-refresh')),
      ),
    ),
  );
  await loaded;
}

// O uso do disco e os itens da pasta aberta.
async function load() {
  const [summary] = await Promise.all([screen.ctx.api.call('offline_summary'), loadFolder()]);
  drawUsage(summary);
}

// Só os itens da pasta aberta (o topo, com a trilha vazia) e a trilha.
async function loadFolder() {
  const folder = screen.trail.at(-1)?.id ?? '';
  const items = await screen.ctx.api.call('offline_items', { folder_id: folder });
  drawCrumbs();
  drawItems(items);
}

function drawUsage(summary) {
  const { t, locale } = screen.ctx;
  const parts = segments(summary);
  const bar = el('div', { class: 'usage-bar', role: 'img', 'aria-hidden': 'true' },
    parts.map(({ key, percent }) => {
      const part = el('span', { class: `segment segment-${key}` });
      part.style.width = `${percent}%`;
      return part;
    }));
  screen.usage.replaceChildren(
    bar,
    el('ul', { class: 'legend' }, parts.map(({ key, bytes }) =>
      el('li', {}, el('span', { class: `dot segment-${key}` }), `${t(`offline-usage-${key}`)} · ${formatSize(bytes, locale)}`))),
  );
}

// "Arquivos off-line › Pasta › Subpasta": cada parte volta para aquela pasta.
function drawCrumbs() {
  const { t } = screen.ctx;
  const steps = [{ id: '', name: t('offline-title') }, ...screen.trail];
  screen.crumbs.replaceChildren(...steps.flatMap((step, i) => {
    const last = i === steps.length - 1;
    const button = el('button', { class: 'text', 'aria-current': last ? 'location' : false, disabled: last }, step.name);
    button.addEventListener('click', () => go(screen.trail.slice(0, i)));
    return i ? [icon('chevron_right'), button] : [button];
  }));
}

function go(trail) {
  screen.trail = trail;
  attempt(loadFolder());
}

function drawItems(items) {
  const { api, t, locale } = screen.ctx;
  if (!items.length) {
    screen.items.replaceChildren(el('p', { class: 'empty' }, t(screen.trail.length ? 'offline-folder-empty' : 'offline-empty')));
    return;
  }
  const rows = items.map((item) => {
    const name = item.is_folder
      ? el('button', { class: 'text name', onclick: () => go([...screen.trail, { id: item.id, name: item.name }]) }, item.name)
      : el('span', { class: 'name', title: item.path }, item.name);
    const state = t(item.pin === 'mixed' ? 'offline-state-mixed' : 'offline-state-on');
    return el(
      'li',
      { 'data-name': item.name },
      el('div', {}, name, el('span', { class: 'meta' }, `${formatSize(item.size, locale)} · ${state}`)),
      el('button', {
        class: 'text remove-offline',
        onclick: () => attempt(api.call('unpin_item', { item_id: item.id }), () => {
          toast(t('offline-removed'));
          return load();
        }),
      }, t('offline-remove')),
    );
  });
  screen.items.replaceChildren(el('ul', { class: 'list offline-items' }, rows));
}

function clearCache() {
  const { api, t, locale } = screen.ctx;
  attempt(api.call('clear_cache'), (freed) => {
    toast(t('offline-cache-cleared', { size: formatSize(freed, locale) }));
    return load();
  });
}

async function clearOffline() {
  const { api, t } = screen.ctx;
  const ok = await confirmDialog({
    title: t('offline-clear-title'),
    body: el('p', {}, t('offline-clear-body')),
    confirm: t('offline-clear-confirm'),
    cancel: t('action-cancel'),
    danger: true,
  });
  if (!ok) return;
  attempt(api.call('clear_offline_files'), () => {
    toast(t('offline-cleared'));
    screen.trail = [];
    return load();
  });
}
