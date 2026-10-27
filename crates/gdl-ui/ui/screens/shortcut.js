// Adicionar atalho ao Drive (aberta pelo menu do gerenciador de arquivos e por "Compartilhados
// comigo", com o id do item): escolher a pasta, a partir do Meu Drive e dos Drives compartilhados
// que aceitam itens novos, e criar o atalho nela.

import { attempt, crumbs, el, icon, stylesheet } from '../components.js';
import { itemArg } from '../router.js';

// A pasta aberta: a última da trilha; sem trilha, a lista de locais.
const folderOf = (trail) => trail.at(-1)?.id ?? '';

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta): a pasta
// aberta continua. `trail` é o caminho até ela (`[{ id, name }]`), vazio na lista de locais.
const screen = { ctx: null, account: '', id: null, trail: [], crumbs: null, list: null, add: null };

export async function render(root, ctx) {
  Object.assign(screen, { ctx, ...itemArg(location.search) });
  const { api, t } = ctx;
  const css = stylesheet('screens/shortcut.css');
  if (!screen.id) {
    await css;
    root.replaceChildren(el('main', { class: 'shortcut' }, el('p', { class: 'empty' }, t('shortcut-missing'))));
    return;
  }
  screen.crumbs = el('nav', { class: 'crumbs', 'aria-label': t('shortcut-title') });
  screen.list = el('ul', { class: 'list pick-list folder-list' });
  screen.add = el('button', { class: 'filled add', onclick: add }, t('shortcut-add'));
  const loaded = loadFolder();
  await css;
  root.replaceChildren(
    el('div', { class: 'pick-screen shortcut' },
      el('main', {},
        el('h1', {}, t('shortcut-title')),
        el('p', { class: 'meta' }, t('shortcut-text')),
        screen.crumbs,
        screen.list),
      el('footer', { class: 'actions save-footer' },
        el('button', { class: 'tonal cancel', onclick: () => api.closeWindow() }, t('action-cancel')),
        screen.add)),
  );
  await loaded;
}

// As pastas da pasta aberta (ou os locais) e a trilha até ela.
async function loadFolder() {
  const { api, t } = screen.ctx;
  const folders = await api.call('folders', { account: screen.account, parent_id: folderOf(screen.trail) });
  const steps = [{ name: t('shortcut-places') }, ...screen.trail];
  screen.crumbs.replaceChildren(...crumbs(steps, (i) => go(screen.trail.slice(0, i))));
  screen.list.replaceChildren(...(folders.length
    ? folders.map(row)
    : [el('li', { class: 'empty' }, t(screen.trail.length ? 'shortcut-no-subfolders' : 'shortcut-no-places'))]));
  screen.add.disabled = !screen.trail.length;
}

function go(trail) {
  screen.trail = trail;
  attempt(loadFolder());
}

function row(folder) {
  return el('li', { 'data-folder': folder.id },
    el('button', { class: 'text folder', onclick: () => go([...screen.trail, { id: folder.id, name: folder.name }]) },
      icon('folder'),
      el('span', { class: 'name' }, folder.name),
      folder.has_children ? icon('chevron_right') : []));
}

// Cria o atalho na pasta aberta e fecha a janela: o atalho aparece na pasta.
function add() {
  const { api } = screen.ctx;
  const call = api.call('add_shortcut', { account: screen.account, item_id: screen.id, folder_id: folderOf(screen.trail) });
  attempt(call, () => api.closeWindow());
}
