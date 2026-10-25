// Compartilhados comigo: quem compartilhou e quando, com "Adicionar atalho" e "Abrir na web".

import { attempt, el, toast } from '../../components.js';

export async function render({ api, t, locale }) {
  const items = await api.call('shared_with_me');
  if (!items.length) return el('p', { class: 'empty' }, t('shared-empty'));
  const date = new Intl.DateTimeFormat(locale, { dateStyle: 'medium' });
  return el(
    'ul',
    { class: 'list' },
    items.map((item) =>
      el(
        'li',
        {},
        el('div', {},
          el('span', { class: 'name' }, item.name),
          el('span', { class: 'meta' }, t('shared-by', { owner: item.owner, date: date.format(item.shared_ms) }))),
        el('div', { class: 'row-actions' },
          el('button', { class: 'text', onclick: () =>
            attempt(api.call('add_shortcut', { item_id: item.id }), () => toast(t('shortcut-added'))) }, t('shared-add-shortcut')),
          el('button', { class: 'text', onclick: () =>
            attempt(api.call('open_item', { item_id: item.id, target: 'web' })) }, t('row-open-web'))),
      ),
    ),
  );
}
