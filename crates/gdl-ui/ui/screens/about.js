// Sobre: versão, licença, o aviso de cliente não oficial, os links do projeto e os softwares de
// terceiros (as bibliotecas Rust, geradas no build, e os recursos do front-end).

import { el, stylesheet } from '../components.js';

const PROJECT = 'https://github.com/junior9283/gdrive-desktop-linux';
// Os recursos do front-end (as licenças completas estão em `third-party/`).
const FRONTEND = [
  { name: 'Inter', version: '4.1', license: 'OFL-1.1' },
  { name: 'Material Symbols', version: '', license: 'Apache-2.0' },
];
const LINKS = ['help', 'privacy', 'terms'];

export async function render(root, { api, t }) {
  const css = stylesheet('screens/about.css');
  const [version, crates] = await Promise.all([api.version(), api.thirdParty(), css]);
  const all = [...FRONTEND, ...crates];
  root.replaceChildren(
    el(
      'main',
      { class: 'about' },
      el('img', { class: 'about-logo', src: 'icons/app.png', alt: '' }),
      el('h1', {}, t('about-name')),
      el('p', { class: 'about-version' }, t('about-version', { version })),
      el('p', { class: 'about-license' }, t('about-license')),
      el('p', { class: 'about-unofficial' }, t('about-unofficial')),
      el('p', { class: 'about-project' }, t('about-project', { url: PROJECT })),
      el('div', { class: 'about-links' },
        LINKS.map((name) => el('button', { class: 'text', onclick: () => api.openLink(name) }, t(`link-${name}`)))),
      el('details', { class: 'third-party' },
        el('summary', {}, t('about-third-party', { count: all.length })),
        el('ul', { class: 'licenses' },
          all.map((item) => el('li', {},
            el('span', {}, `${item.name} ${item.version}`.trim()),
            el('span', { class: 'meta' }, item.license || t('about-license-file')))))),
      el('div', { class: 'actions' },
        el('button', { class: 'text about-uninstall', onclick: () => api.openScreen('uninstall') }, t('uninstall-open')),
        el('button', { class: 'filled', onclick: () => api.closeWindow() }, t('action-close'))),
    ),
  );
}
