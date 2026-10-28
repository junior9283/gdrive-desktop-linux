// Drives compartilhados da conta: os ligados abrem na pasta (montagem) ou na web; os desligados
// ficam fora da montagem e se ligam em "Gerenciar drives compartilhados".

import { attempt, el } from '../../components.js';
import { currentAccount } from '../../state.js';

export async function render({ api, t }) {
  const drives = await api.call('shared_drives');
  const action = (drive, cls, target, label) =>
    el('button', {
      class: `text ${cls}`,
      disabled: !drive.enabled,
      onclick: () => attempt(api.call('open_item', { item_id: drive.id, target })),
    }, t(label));
  const list = drives.length
    ? el('ul', { class: 'list drives' },
      drives.map((drive) =>
        el('li', {},
          el('div', {},
            el('span', { class: 'name' }, drive.name),
            drive.enabled ? [] : el('span', { class: 'meta' }, t('drives-off'))),
          el('div', { class: 'row-actions' },
            action(drive, 'open-folder', 'file', 'drives-open-folder'),
            action(drive, 'open-web', 'web', 'row-open-web')))))
    : el('p', { class: 'empty' }, t('drives-empty'));
  return el(
    'div',
    {},
    list,
    el('button', { class: 'tonal manage-shared-drives', onclick: () =>
      api.openScreen('shared-drives', currentAccount()) }, t('more-shared-drives')),
  );
}
