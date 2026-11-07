// Sobre: versão, licença, o aviso de cliente não oficial, os links do projeto e os softwares de
// terceiros (as bibliotecas Rust, geradas no build, e os recursos do front-end).

import { appLogo, el, stylesheet } from '../components.js';
import { updateBanner } from './about/model.js';

const PROJECT = 'https://github.com/junior9283/gdrive-desktop-linux';
// Os recursos do front-end (as licenças completas estão em `third-party/`).
const FRONTEND = [
  { name: 'Inter', version: '4.1', license: 'OFL-1.1' },
  { name: 'Material Symbols', version: '', license: 'Apache-2.0' },
];
const LINKS = ['help', 'privacy', 'terms'];
// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta): a faixa
// da tela atual, que o sinal `update-changed` refaz.
const screen = { update: null, ctx: null, listening: null };

// A faixa da versão nova (escondida sem ela), a partir do `UpdateInfo` (`null` se falhou).
function drawUpdate(box, info, { api, t }) {
  const banner = updateBanner(info);
  const buttons = {
    update: () => el('button', { class: 'filled', onclick: () => api.call('start_update').catch(() => {}) }, t('about-update-now')),
    release: () => el('button', { class: 'text', onclick: () => api.call('open_release').catch(() => {}) }, t('about-update-release')),
  };
  box.hidden = !banner;
  box.replaceChildren(...(banner
    ? [
      el('p', {}, t(banner.text, { version: banner.version })),
      el('div', { class: 'about-update-actions' }, banner.actions.map((a) => buttons[a]())),
    ]
    : []));
}

const updateInfo = (api) => api.call('update_info').catch(() => null);

export async function render(root, { api, official, t }) {
  const css = stylesheet('screens/about.css');
  const [version, crates, info] = await Promise.all([api.version(), api.thirdParty(), updateInfo(api), css]);
  const all = [...FRONTEND, ...crates];
  const update = el('div', { class: 'about-update' });
  drawUpdate(update, info, { api, t });
  Object.assign(screen, { update, ctx: { api, t } });
  screen.listening ??= api.on('update-changed', async () => {
    const latest = await updateInfo(screen.ctx.api);
    drawUpdate(screen.update, latest, screen.ctx);
  });
  root.replaceChildren(
    el(
      'main',
      { class: 'about' },
      el('img', { class: 'about-logo', src: appLogo(official), alt: '' }),
      el('h1', {}, t('about-name')),
      el('p', { class: 'about-version' }, t('about-version', { version })),
      update,
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
