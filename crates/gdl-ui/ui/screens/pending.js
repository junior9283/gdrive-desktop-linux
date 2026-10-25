// As telas que chegam nas próximas tarefas.

import { el } from '../components.js';

export function render(root, { t }) {
  root.replaceChildren(el('main', { class: 'pending' }, el('p', {}, t('screen-not-ready'))));
}
