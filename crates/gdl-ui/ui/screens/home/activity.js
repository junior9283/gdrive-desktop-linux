// Atividade de sincronização: tabela ordenável e paginada, com o menu de cada linha.

import { attempt, el, icon, popupMenu, toast } from '../../components.js';
import { formatAgo, formatSize } from '../../format.js';
import { paginate, sortRows } from './model.js';

const SIZES = [10, 25, 50];
// Os envios que ainda dá para cancelar.
const CANCELABLE = new Set(['upload-queued', 'uploading']);

// Ordem e página continuam ao recarregar (os eventos de atividade chegam o tempo todo).
const table = { key: 'time', dir: 'desc', page: 1, size: 10 };

export async function render(ctx) {
  const rows = await ctx.api.call('activity');
  const view = el('div', { class: 'activity' });
  const draw = () => view.replaceChildren(...content(ctx, rows, draw));
  draw();
  return view;
}

function content(ctx, rows, draw) {
  const { t, locale } = ctx;
  if (!rows.length) return [el('p', { class: 'empty' }, t('activity-empty'))];
  const page = paginate(sortRows(rows, table.key, table.dir), table.page, table.size);
  table.page = page.page;
  const sortBy = (key) => {
    table.dir = table.key === key && table.dir === 'asc' ? 'desc' : 'asc';
    table.key = key;
    draw();
  };
  const heading = (key, label) =>
    el(
      'th',
      { 'aria-sort': table.key === key ? (table.dir === 'asc' ? 'ascending' : 'descending') : 'none' },
      el('button', { class: 'sort', onclick: () => sortBy(key) }, label),
    );
  const now = Date.now();
  const body = page.rows.map((row) => {
    const menu = el('button', { class: 'icon-button', 'aria-label': t('row-menu') }, icon('more_vert'));
    menu.addEventListener('click', () => rowMenu(menu, row, ctx));
    const state = row.percent >= 0 && row.state === 'uploading'
      ? t('activity-uploading-percent', { percent: row.percent })
      : t(`activity-${row.state}`);
    return el(
      'tr',
      {},
      el('td', { class: 'name', title: row.path }, row.name),
      el('td', {}, formatSize(row.size, locale)),
      el('td', {}, formatAgo(row.time_ms, now, locale)),
      el('td', { class: 'state' }, state),
      el('td', {}, menu),
    );
  });
  const size = el(
    'select',
    { 'aria-label': t('page-size') },
    SIZES.map((n) => el('option', { value: n, selected: n === table.size }, String(n))),
  );
  size.addEventListener('change', () => {
    table.size = Number(size.value);
    table.page = 1;
    draw();
  });
  const go = (delta) => () => {
    table.page += delta;
    draw();
  };
  return [
    el(
      'table',
      { class: 'activity-table' },
      el(
        'thead',
        {},
        el('tr', {}, heading('name', t('column-name')), heading('size', t('column-size')),
          heading('time', t('column-date')), el('th', {}, t('column-status')), el('th', {})),
      ),
      el('tbody', {}, body),
    ),
    el(
      'div',
      { class: 'pager' },
      el('label', {}, t('page-size'), size),
      el('span', {}, t('showing', page)),
      el('button', { class: 'icon-button', 'aria-label': t('page-previous'), disabled: page.page <= 1, onclick: go(-1) }, icon('chevron_left')),
      el('button', { class: 'icon-button', 'aria-label': t('page-next'), disabled: page.page >= page.pages, onclick: go(1) }, icon('chevron_right')),
    ),
  ];
}

function rowMenu(anchor, row, { api, t }) {
  const id = row.item_id;
  popupMenu(anchor, [
    { id: 'open-folder', label: t('row-open-folder'), disabled: !id,
      action: () => attempt(api.call('open_item', { item_id: id, target: 'folder' })) },
    { id: 'open-web', label: t('row-open-web'), disabled: !id,
      action: () => attempt(api.call('open_item', { item_id: id, target: 'web' })) },
    { id: 'copy-link', label: t('row-copy-link'), disabled: !id,
      action: () => attempt(
        api.call('web_link', { item_id: id }).then((url) => api.copy(url)),
        () => toast(t('link-copied')),
      ) },
    { id: 'cancel-upload', label: t('row-cancel-upload'), disabled: !CANCELABLE.has(row.state),
      action: () => attempt(api.call('cancel_upload', { item_id: id }), (result) =>
        toast(t(result === 'too-late' ? 'upload-finished-first' : 'upload-canceled'))) },
  ]);
}
