// Sobre: versão, licença e o aviso de cliente não oficial.

import { el } from '../components.js';

const PROJECT = 'https://github.com/junior9283/gdrive-desktop-linux';

export async function render(root, { api, i18n }) {
  const version = await api.version();
  root.replaceChildren(
    el(
      'main',
      { class: 'about' },
      el('img', { class: 'about-logo', src: 'icons/app.svg', alt: '' }),
      el('h1', {}, i18n.t('about-name')),
      el('p', { class: 'about-version' }, i18n.t('about-version', { version })),
      el('p', { class: 'about-license' }, i18n.t('about-license')),
      el('p', { class: 'about-unofficial' }, i18n.t('about-unofficial')),
      el('p', { class: 'about-project' }, i18n.t('about-project', { url: PROJECT })),
      el('div', { class: 'actions' },
        el('button', { class: 'filled', onclick: () => api.closeWindow() }, i18n.t('action-close'))),
    ),
  );
}
