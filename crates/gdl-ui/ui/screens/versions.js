// Gerenciar versões (aberta pelo menu do gerenciador de arquivos com o id do item): as versões
// guardadas do arquivo, da atual para a mais antiga, cada uma com baixar, restaurar e "manter
// para sempre" no menu da linha. A lógica sem DOM fica em `versions/model.js`.

import { attempt, confirmDialog, el, menuButton, stylesheet, toast } from '../components.js';
import { formatDate, formatSize } from '../format.js';
import { itemArg } from '../router.js';
import { versionActions } from './versions/model.js';

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta).
const screen = { ctx: null, root: null, account: '', id: null };

export async function render(root, ctx) {
  Object.assign(screen, { ctx, root, ...itemArg(location.search) });
  const css = stylesheet('screens/versions.css');
  if (!screen.id) {
    await css;
    root.replaceChildren(el('main', { class: 'versions' }, el('p', { class: 'empty' }, ctx.t('versions-missing'))));
    return;
  }
  const [data] = await Promise.all([load(), css]);
  draw(data);
}

// O item da tela, nos argumentos de toda chamada.
const item = () => ({ account: screen.account, item_id: screen.id });

const load = () => screen.ctx.api.call('versions', item());

// Faz a mudança, avisa e redesenha com o que o Drive devolveu.
const change = (promise, done) =>
  attempt(promise.then(load), (data) => {
    toast(done);
    draw(data);
  });

function draw(data) {
  const { api, t } = screen.ctx;
  screen.root.replaceChildren(
    el(
      'main',
      { class: 'versions' },
      el('h1', { class: 'name' }, t('versions-title', { name: data.name })),
      el('p', { class: 'meta' }, t('versions-text')),
      el('ul', { class: 'list version-list' }, data.versions.map((version) => row(version, data))),
      el('div', { class: 'actions' }, el('button', { class: 'filled', onclick: () => api.closeWindow() }, t('action-close'))),
    ),
  );
}

function row(version, data) {
  const { t, locale } = screen.ctx;
  const details = [
    version.size >= 0 ? formatSize(version.size, locale) : null,
    version.author || null,
    version.keep_forever ? t('versions-kept') : null,
  ].filter(Boolean);
  return el(
    'li',
    { 'data-version': version.id },
    el('div', {},
      el('span', { class: 'name' },
        formatDate(version.modified_ms, locale),
        version.current ? el('span', { class: 'chip' }, t('versions-current')) : []),
      el('span', { class: 'meta' }, details.join(' · '))),
    menuButton(t('versions-actions'), () =>
      versionActions(version, data.can_edit).map((action) => ({
        id: action,
        label: t(`versions-${action}`),
        action: () => run(action, version),
      }))),
  );
}

async function run(action, version) {
  const { api, t, locale } = screen.ctx;
  const args = { ...item(), version_id: version.id };
  if (action === 'download') {
    attempt(api.call('version_download', args), (path) => toast(t('versions-downloaded', { path })));
  } else if (action === 'restore') {
    const ok = await confirmDialog({
      title: t('versions-restore-title'),
      body: el('p', {}, t('versions-restore-body', { date: formatDate(version.modified_ms, locale) })),
      confirm: t('versions-restore'),
      cancel: t('action-cancel'),
    });
    if (ok) change(api.call('version_restore', args), t('versions-restored'));
  } else {
    const keep = action === 'keep';
    change(api.call('version_keep', { ...args, keep }), t(keep ? 'versions-kept-done' : 'versions-unkept-done'));
  }
}
