// Lista de erros: os erros de sincronização da conta mostrada agrupados por categoria (cada grupo
// com a explicação e 10 itens por página), o menu de cada item (Abrir pasta, Tentar de novo), uma
// faixa para cada outra conta com erros (Trocar de conta) e Concluído. A lógica fica em
// `errors/model.js`.

import { attempt, el, menuButton, pager, stylesheet } from '../components.js';
import { paginate } from './home/model.js';
import { groupErrors, otherAccountErrors } from './errors/model.js';
import { onAccountChange, setCurrentAccount } from '../state.js';

const PAGE_SIZE = 10;
// Os erros mudam com o status; no máximo uma releitura a cada meio segundo.
const SETTLE_MS = 500;

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta): a
// página de cada grupo continua ao reler.
const screen = { ctx: null, main: null, pages: {}, shown: null, timer: null, wired: false };

export async function render(root, ctx) {
  const css = stylesheet('screens/errors.css');
  screen.ctx = ctx;
  const { api, t } = ctx;
  screen.main = el('main', { class: 'groups' });
  if (!screen.wired) {
    screen.wired = true;
    const reload = () => {
      screen.timer ??= setTimeout(() => {
        screen.timer = null;
        // Escondida, não relê; ao voltar, relê.
        if (!document.hidden) load().catch(() => {});
      }, SETTLE_MS);
    };
    api.on('status-changed', reload);
    onAccountChange(() => {
      screen.pages = {};
      load(true).catch(() => {});
    });
    document.addEventListener('visibilitychange', reload);
  }
  const loaded = load(true);
  await css;
  root.replaceChildren(
    el(
      'div',
      { class: 'errors' },
      el('h1', {}, t('errors-title')),
      screen.main,
      el('div', { class: 'actions' }, el('button', { class: 'filled done', onclick: () => api.closeWindow() }, t('errors-done'))),
    ),
  );
  await loaded;
}

// Relê os erros e redesenha, se mudaram (redesenhar fecharia o menu aberto de um item).
async function load(force = false) {
  const { api, t } = screen.ctx;
  const [errors, status] = await Promise.all([api.call('errors'), api.call('status')]);
  const others = otherAccountErrors(status.accounts, status.account);
  const shown = JSON.stringify([errors, others]);
  if (!force && shown === screen.shown) return;
  screen.shown = shown;
  const groups = groupErrors(errors);
  screen.main.replaceChildren(
    ...others.map(otherAccount),
    ...(groups.length ? groups.map(group) : [el('p', { class: 'empty' }, t('errors-empty'))]),
  );
}

// "Há N erros em <e-mail>", com Trocar de conta (passa a mostrar os erros dela).
function otherAccount({ email, errors }) {
  const { t } = screen.ctx;
  return el('div', { class: 'banner info other-account', role: 'status' },
    el('span', {}, t('errors-other-account', { count: errors, email })),
    el('button', { class: 'text switch-account', onclick: () => setCurrentAccount(email) }, t('errors-switch-account')));
}

function group({ category, items }) {
  const { t } = screen.ctx;
  const section = el('section', { class: 'error-group', 'data-category': category });
  const draw = () => {
    const page = paginate(items, screen.pages[category] ?? 1, PAGE_SIZE);
    const go = (delta) => {
      screen.pages[category] = page.page + delta;
      draw();
    };
    section.replaceChildren(
      el('h2', {}, t(`error-category-${category}`), el('span', { class: 'meta' }, ` · ${t('errors-count', { count: items.length })}`)),
      el('p', { class: 'meta' }, t(`error-category-${category}-text`)),
      el('ul', { class: 'list' }, page.rows.map(row)),
      page.pages > 1 ? pager(t, page, go) : [],
    );
  };
  draw();
  return section;
}

function row(item) {
  const { api, t } = screen.ctx;
  const id = item.item_id;
  const menu = menuButton(t('row-menu'), () => [
    { id: 'open-folder', label: t('row-open-folder'), disabled: !id,
      action: () => attempt(api.call('open_item', { item_id: id, target: 'folder' })) },
    { id: 'retry', label: t('action-retry'), disabled: !id,
      action: () => attempt(api.call('retry', { item_id: id }), () => load()) },
  ]);
  return el(
    'li',
    { title: item.message },
    el('div', {}, el('span', { class: 'name' }, item.name), el('span', { class: 'meta name' }, item.path)),
    menu,
  );
}
