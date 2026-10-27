// Compartilhados comigo: quem compartilhou e quando, com "Adicionar atalho" (a tela de escolher a
// pasta) e "Abrir na web".

import { attempt, el } from '../../components.js';
import { currentAccount } from '../../state.js';

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
          el('button', { class: 'text add-shortcut', onclick: () =>
            api.openScreen('shortcut', `${currentAccount()}/${item.id}`) }, t('shared-add-shortcut')),
          el('button', { class: 'text', onclick: () =>
            attempt(api.call('open_item', { item_id: item.id, target: 'web' })) }, t('row-open-web'))),
      ),
    ),
  );
}
