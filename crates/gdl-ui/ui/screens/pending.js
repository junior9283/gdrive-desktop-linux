// As telas que chegam nas próximas tarefas.

import { el } from '../components.js';

export function render(root, { i18n }) {
  root.replaceChildren(el('main', { class: 'pending' }, el('p', {}, i18n.t('screen-not-ready'))));
}
