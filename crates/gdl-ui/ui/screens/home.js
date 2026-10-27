// Janela principal: cabeçalho com o estado da sincronização, menu da conta, "Mais opções", o menu
// lateral com as visões e a área da visão escolhida. Atualiza com os eventos da ponte.

import { attempt, el, icon, stylesheet } from '../components.js';
import { accountButton, accountMenu } from './home/account.js';
import { headerFor, shownStatus, withoutAccount } from './home/model.js';
import { moreMenu } from './home/more.js';
import * as activity from './home/activity.js';
import * as links from './home/links.js';
import * as notifications from './home/notifications.js';
import * as offline from './home/offline.js';
import * as shared from './home/shared.js';
import * as start from './home/start.js';
import { currentAccount, onAccountChange, setCurrentAccount } from '../state.js';

const VIEWS = { start, activity, offline, shared, notifications, links };
// Tudo de uma vez: o login que terminou e a conta mostrada que mudou.
const EVERYTHING = { status: true, badge: true, views: Object.keys(VIEWS) };
// O que cada evento da ponte atualiza: o cabeçalho (`status`), o selo das notificações (`badge`)
// e as visões que mostram o que mudou.
const EVENTS = {
  'status-changed': { status: true, views: ['start'] },
  'activity-changed': { views: ['activity'] },
  'notifications-changed': { badge: true, views: ['notifications'] },
  'login-finished': EVERYTHING,
};
// Durante a sincronização os eventos chegam várias vezes por segundo: no máximo uma atualização
// a cada meio segundo, com o que se juntou no intervalo.
const SETTLE_MS = 500;

// O estado da janela entre as chamadas de `render` (ela é chamada de novo quando o serviço volta).
const home = { view: 'start', status: null, ctx: null, wired: false };
// Os pedaços da página que as atualizações reescrevem.
const ui = {};
// O que falta atualizar e o agendamento da próxima atualização.
const due = { status: false, badge: false, view: false };
const pace = { timer: null, running: false, last: 0 };

export async function render(root, ctx) {
  home.ctx = ctx;
  if (!home.wired) wire(ctx);
  const css = stylesheet('screens/home.css');
  const page = layout(ctx);
  await css;
  root.replaceChildren(page);
  await update({ status: true, badge: true, view: true });
}

function layout({ api, t }) {
  ui.title = el('h1', { class: 'header-title' });
  ui.subtitle = el('p', { class: 'header-subtitle' });
  ui.pause = el('button', { class: 'header-pause tonal', onclick: togglePause });
  ui.signIn = el('button', {
    class: 'header-sign-in filled',
    hidden: true,
    onclick: () => api.openScreen('onboarding', `resume:${home.status.account}`),
  }, t('account-sign-in-again'));
  const search = el('input', {
    class: 'search',
    type: 'search',
    placeholder: t('search-placeholder'),
    'aria-label': t('search-placeholder'),
  });
  search.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && search.value.trim()) api.openScreen('search', search.value.trim());
  });
  ui.header = el(
    'section',
    { class: 'header', tabindex: '-1', 'aria-live': 'polite' },
    el('div', { class: 'header-text' }, ui.title, ui.subtitle),
    ui.pause,
    ui.signIn,
  );
  ui.account = el('button', { class: 'account-button icon-button', 'aria-label': t('account-menu') });
  ui.account.addEventListener('click', () => accountMenu(ui.account, context()));
  const more = el('button', { class: 'more-button icon-button', 'aria-label': t('more-options') }, icon('more_vert'));
  more.addEventListener('click', () => moreMenu(more, context()));
  ui.nav = el(
    'nav',
    { class: 'nav' },
    Object.keys(VIEWS).map((name) =>
      el(
        'button',
        { 'data-view': name, onclick: () => show(name) },
        el('span', {}, t(`view-${name}`)),
        name === 'notifications' ? el('span', { class: 'badge', hidden: true }) : [],
      ),
    ),
  );
  ui.main = el('main', { class: 'view' });
  return el(
    'div',
    { class: 'home' },
    el('div', { class: 'topbar' }, search, ui.account, more),
    ui.header,
    el('div', { class: 'body' }, ui.nav, ui.main),
  );
}

// O que as visões e os menus recebem.
function context() {
  return { ...home.ctx, home: { status: home.status, show, setUnread } };
}

function wire({ api }) {
  home.wired = true;
  for (const [event, work] of Object.entries(EVENTS)) api.on(event, () => schedule(work));
  // Outra conta escolhida (aqui ou em outra janela): tudo é lido de novo.
  onAccountChange(() => schedule(EVERYTHING));
  // Escondida, a janela não se atualiza; ao voltar, faz o que ficou para trás.
  document.addEventListener('visibilitychange', () => schedule({}));
  // "Há N minutos" anda sozinho.
  setInterval(() => home.status && !document.hidden && updateHeader(), 30_000);
  // Ctrl+Shift+Alt+G leva o foco para o painel de status.
  document.addEventListener('keydown', (e) => {
    if (e.ctrlKey && e.shiftKey && e.altKey && e.key.toLowerCase() === 'g') {
      e.preventDefault();
      ui.header?.focus();
    }
  });
}

// Junta `work` ao que falta e agenda a atualização (uma por vez, com `SETTLE_MS` entre elas).
function schedule({ status = false, badge = false, views = [] }) {
  due.status ||= status;
  due.badge ||= badge;
  due.view ||= views.includes(home.view);
  if (pace.timer || pace.running || !(due.status || due.badge || due.view)) return;
  pace.timer = setTimeout(flush, Math.max(0, pace.last + SETTLE_MS - Date.now()));
}

async function flush() {
  pace.timer = null;
  if (document.hidden) return;
  const work = { ...due };
  Object.assign(due, { status: false, badge: false, view: false });
  pace.running = true;
  try {
    await update(work);
  } catch {
    // Sem serviço o aviso aparece; o próximo evento tenta de novo.
  } finally {
    pace.running = false;
    pace.last = Date.now();
    schedule({});
  }
}

// O cabeçalho primeiro: o selo e a visão dependem do `Status` novo.
async function update({ status, badge, view }) {
  if (status) await updateStatus();
  await Promise.all([badge && updateBadge(), view && showView()]);
}

async function updateStatus() {
  const { api } = home.ctx;
  // Pedir a primeira conta traz a lista de todas: com ela se confere a conta guardada.
  let status = await api.call('status', { account: '' });
  const shown = currentAccount();
  if (shown && !status.accounts.some((a) => a.email === shown)) {
    // A conta mostrada foi desconectada: fica a primeira.
    setCurrentAccount('');
  } else if (shown && shown !== status.account) {
    status = await api.call('status');
  }
  home.status = shownStatus(status);
  updateHeader();
}

function updateHeader() {
  const { t, locale } = home.ctx;
  const status = home.status;
  const header = headerFor(status, Date.now(), locale);
  ui.title.textContent = t(...header.title);
  ui.subtitle.textContent = header.subtitle ? t(...header.subtitle) : '';
  ui.pause.hidden = !header.pause;
  if (header.pause) ui.pause.textContent = t(header.pause);
  ui.signIn.hidden = !header.signIn;
  ui.nav.hidden = withoutAccount(status.state);
  accountButton(ui.account, home.ctx.api, status.account);
  ui.header.dataset.state = status.state;
}

function setUnread(count) {
  const badge = ui.nav.querySelector('.badge');
  badge.hidden = count === 0;
  badge.textContent = String(count);
}

// Com a visão das notificações aberta, é ela que acerta o selo (ela marca tudo como lido).
async function updateBadge() {
  if (withoutAccount(home.status?.state) || home.view === 'notifications') return;
  try {
    setUnread((await home.ctx.api.call('notifications')).filter((n) => !n.read).length);
  } catch {
    setUnread(0);
  }
}

function togglePause() {
  attempt(home.ctx.api.call(home.status.paused ? 'resume' : 'pause').then(updateStatus));
}

function show(name) {
  home.view = name;
  showView();
}

async function showView() {
  const { t, api } = home.ctx;
  for (const button of ui.nav.querySelectorAll('button')) {
    button.classList.toggle('active', button.dataset.view === home.view);
  }
  // Sem conta de pé: reconectando as guardadas só avisa; sem nenhuma, oferece o login.
  const state = home.status?.state;
  if (withoutAccount(state)) {
    ui.main.replaceChildren(
      el(
        'div',
        { class: 'card signed-out' },
        el('p', {}, t(`home-${state}`)),
        state === 'signed-out'
          ? el('button', { class: 'filled', onclick: () => api.openScreen('onboarding') }, t('sign-in'))
          : [],
      ),
    );
    return;
  }
  const view = home.view;
  try {
    const content = await VIEWS[view].render(context());
    // Outra visão pode ter sido escolhida enquanto esta carregava.
    if (view === home.view) ui.main.replaceChildren(content);
  } catch (error) {
    if (error?.name === 'Unavailable') return;
    ui.main.replaceChildren(el('p', { class: 'view-error' }, error?.message ?? String(error)));
  }
}
