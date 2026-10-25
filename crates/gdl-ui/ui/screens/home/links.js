// Links rápidos: o Drive na web e a documentação do projeto.

import { el } from '../../components.js';

const LINKS = ['drive-web', 'faq', 'help'];

export async function render({ api, t }) {
  return el(
    'ul',
    { class: 'list' },
    LINKS.map((name) =>
      el('li', {}, el('button', { class: 'text', onclick: () => api.openLink(name) }, t(`link-${name}`))),
    ),
  );
}
