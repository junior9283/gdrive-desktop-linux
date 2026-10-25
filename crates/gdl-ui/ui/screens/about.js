// Sobre: versão, licença e o aviso de cliente não oficial.

import { el } from '../components.js';

const PROJECT = 'https://github.com/junior9283/gdrive-desktop-linux';

export async function render(root, { api, t }) {
  const version = await api.version();
  root.replaceChildren(
    el(
      'main',
      { class: 'about' },
      el('img', { class: 'about-logo', src: 'icons/app.svg', alt: '' }),
      el('h1', {}, t('about-name')),
      el('p', { class: 'about-version' }, t('about-version', { version })),
      el('p', { class: 'about-license' }, t('about-license')),
      el('p', { class: 'about-unofficial' }, t('about-unofficial')),
      el('p', { class: 'about-project' }, t('about-project', { url: PROJECT })),
      el('div', { class: 'actions' },
        el('button', { class: 'filled', onclick: () => api.closeWindow() }, t('action-close'))),
    ),
  );
}
