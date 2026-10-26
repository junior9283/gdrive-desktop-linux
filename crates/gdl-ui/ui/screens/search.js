// Pesquisa rápida: janela flutuante com o campo e os resultados (ícone, nome e pasta). Campo
// vazio mostra os recentes; Enter ou clique abre o item pela montagem e fecha a janela; setas
// andam pela lista; Esc e a perda de foco fecham. A lógica fica em `search/model.js`.

import { el, errorText, icon, stylesheet, toastError } from '../components.js';
import { screenArg } from '../router.js';
import { debounce, folderOf, iconFor, moveSelection, shownFor } from './search/model.js';

// Espera entre teclas antes de pesquisar.
const TYPING_MS = 250;

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta).
const screen = { ctx: null, field: null, list: null, mount: '', shown: null, results: [], rows: [], selected: -1, asked: 0, wired: false };

export async function render(root, ctx) {
  const css = stylesheet('screens/search.css');
  screen.ctx = ctx;
  const { t } = ctx;
  screen.field = el('input', {
    class: 'search-field',
    type: 'search',
    role: 'combobox',
    placeholder: t('search-placeholder'),
    'aria-label': t('search-placeholder'),
    'aria-controls': 'results',
    'aria-expanded': 'true',
    autocomplete: 'off',
    spellcheck: 'false',
    value: screen.field?.value ?? screenArg(location.search) ?? '',
  });
  screen.list = el('div', { id: 'results', class: 'results', role: 'listbox' });
  const typed = debounce(() => find(screen.field.value), TYPING_MS);
  screen.field.addEventListener('input', typed);
  screen.field.addEventListener('keydown', (e) => keydown(e, typed));
  if (!screen.wired) wire(ctx);
  // A montagem (para mostrar as pastas) e a primeira pesquisa vão juntas.
  const asked = ++screen.asked;
  const [status, answer] = await Promise.all([ctx.api.call('status'), ask(screen.field.value), css]);
  screen.mount = status.mount_point;
  root.replaceChildren(el('div', { class: 'search' }, el('div', { class: 'search-box' }, icon('search'), screen.field), screen.list));
  screen.field.focus();
  if (asked === screen.asked) draw(answer);
}

function wire({ api }) {
  screen.wired = true;
  // A bandeja ou a principal pediram a pesquisa de novo, talvez com outro texto.
  api.on('screen-arg', (text) => {
    screen.field.value = text;
    find(text);
  });
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') api.closeWindow();
  });
  // Como um menu: fora de foco, some.
  window.addEventListener('blur', () => api.closeWindow());
}

function keydown(e, typed) {
  const moves = { ArrowDown: 1, ArrowUp: -1 };
  if (moves[e.key]) {
    e.preventDefault();
    select(moveSelection(screen.selected, moves[e.key], screen.results.length));
  } else if (e.key === 'Enter') {
    e.preventDefault();
    typed.cancel();
    enter();
  }
}

// Abre o escolhido pelas setas (ou o primeiro). Enter logo depois de digitar: a lista ainda é a
// do texto anterior, então antes pesquisa o texto do campo (a lista nova vem sem escolha).
async function enter() {
  if (!shownFor(screen.field.value, screen.shown)) await find(screen.field.value);
  if (shownFor(screen.field.value, screen.shown)) open(screen.results[Math.max(0, screen.selected)]);
}

// Pesquisa `query` (vazia: os recentes): `{ text, results }` ou, se falhou, `{ text, error }`.
async function ask(query) {
  const { api } = screen.ctx;
  const text = query.trim();
  try {
    return { text, results: text ? await api.call('search', { query: text }) : await api.call('recent_items') };
  } catch (error) {
    return { text, results: [], error };
  }
}

// Pesquisa e desenha o que voltar, se nada mais novo foi pedido.
async function find(query) {
  const asked = ++screen.asked;
  const answer = await ask(query);
  if (asked === screen.asked) draw(answer);
}

// Os resultados, com o aviso que couber: os recentes, nada encontrado ou a falha.
function draw({ text, results, error }) {
  const { t } = screen.ctx;
  let notice = null;
  if (error) notice = el('p', { class: 'empty' }, t('search-failed', { error: errorText(error) }));
  else if (!text) notice = el('h2', { class: 'results-heading' }, t('search-recent'));
  else if (!results.length) notice = el('p', { class: 'empty' }, t('search-none', { query: text }));
  const rows = results.map((result, i) =>
    el(
      'div',
      { class: 'result', role: 'option', id: `result-${i}`, 'aria-selected': 'false', onclick: () => open(result) },
      icon(iconFor(result.mime, result.name)),
      el('div', { class: 'result-text' },
        el('span', { class: 'name' }, result.name),
        el('span', { class: 'meta name' }, folderOf(result.path, screen.mount))),
    ));
  Object.assign(screen, { shown: text, results, rows, selected: -1 });
  screen.list.replaceChildren(...(notice ? [notice] : []), ...rows);
}

function select(index) {
  screen.selected = index;
  screen.rows.forEach((row, i) => row.setAttribute('aria-selected', String(i === index)));
  const row = screen.rows[index];
  screen.field.setAttribute('aria-activedescendant', row?.id ?? '');
  row?.scrollIntoView({ block: 'nearest' });
}

// Abre o item (a pasta, se for uma) pela montagem e fecha a janela.
function open(result) {
  if (!result) return;
  const { api } = screen.ctx;
  api.call('open_item', { item_id: result.id, target: 'file' }).then(() => api.closeWindow(), toastError);
}
