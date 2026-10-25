// Arquivos off-line: os primeiros itens e o caminho para a tela completa.

import { el } from '../../components.js';
import { formatSize } from '../../format.js';
import { offlineText } from './model.js';

const SHOWN = 5;

export async function render({ api, t, locale }) {
  const [summary, items] = await Promise.all([
    api.call('offline_summary'),
    api.call('offline_items', { folder_id: '' }),
  ]);
  return el(
    'div',
    {},
    el('p', {}, t(...offlineText(summary, locale))),
    items.length
      ? el('ul', { class: 'list' }, items.slice(0, SHOWN).map((item) =>
          el('li', {}, el('span', { class: 'name', title: item.path }, item.name),
            el('span', { class: 'meta' }, formatSize(item.size, locale)))))
      : el('p', { class: 'empty' }, t('offline-empty')),
    el('button', { class: 'filled', onclick: () => api.openScreen('offline') }, t('offline-manage')),
  );
}
