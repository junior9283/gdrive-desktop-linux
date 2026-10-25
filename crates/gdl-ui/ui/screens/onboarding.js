// Primeiro uso: os passos numa lista à esquerda e o passo atual à direita, com Pular, Voltar e
// Continuar. A lógica dos passos fica em `onboarding/model.js`; aqui, o DOM e as chamadas.

import { prefValue } from '../api.js';
import { attempt, card, el, errorText, stylesheet, toast } from '../components.js';
import { actions, firstState, nextStep, stepsFor } from './onboarding/model.js';

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta).
const screen = { root: null, ctx: null, state: null, listening: null };

export async function render(root, ctx) {
  const css = stylesheet('screens/onboarding.css');
  Object.assign(screen, { root, ctx });
  screen.listening ??= ctx.api.on('login-finished', async ({ ok, error }) => {
    // A pasta do Drive existe a partir do login.
    const mount = ok ? (await ctx.api.call('status').catch(() => null))?.mount_point : undefined;
    dispatch({ type: 'login-finished', ok, error, mount });
  });
  if (!screen.state) {
    const [status, prefs] = await Promise.all([ctx.api.call('status'), ctx.api.call('preferences')]);
    screen.state = firstState({
      signedIn: status.state !== 'signed-out',
      loginOnly: prefValue(prefs, 'DisableOnboardingDialog', false) === true,
      mount: status.mount_point,
    });
  }
  await css;
  draw();
}

function dispatch(event) {
  const before = screen.state.step;
  screen.state = nextStep(screen.state, event);
  const moved = screen.state.step !== before;
  if (moved && screen.state.step === 'waiting') beginLogin();
  if (event.type !== 'edit') draw(moved);
}

// Pede o login ao serviço (ele abre o navegador); o fim chega pelo evento `login-finished`.
function beginLogin() {
  const { clientId, secret } = screen.state;
  screen.ctx.api
    .call('begin_login', { client_id: clientId.trim(), client_secret: secret.trim() })
    .then((url) => dispatch({ type: 'login-url', url }))
    .catch((e) => dispatch({ type: 'login-finished', ok: false, error: errorText(e) }));
}

// Desenha o passo atual; o foco vai para o primeiro campo (ou para Continuar) só ao trocar de
// passo. No fim, fecha a janela.
function draw(moved = true) {
  const { ctx, state } = screen;
  if (state.step === 'end') {
    ctx.api.closeWindow();
    return;
  }
  const { t } = ctx;
  const steps = stepsFor(state.loginOnly);
  const at = steps.indexOf(state.step);
  const list = el(
    'ol',
    { class: 'steps', 'aria-label': t('onboarding-steps') },
    steps.map((step, i) =>
      el('li', { class: i < at ? 'done' : '', 'aria-current': i === at ? 'step' : false }, t(`onboarding-step-${step}`)),
    ),
  );
  screen.root.replaceChildren(
    el('div', { class: 'onboarding' }, list, el('main', { class: 'step' }, ...STEPS[state.step](ctx, state), footer())),
  );
  if (moved) screen.root.querySelector('input, .continue')?.focus();
}

const PRIMARY = { continue: 'onboarding-continue', retry: 'action-retry', finish: 'onboarding-finish' };

// Pular, Voltar e o botão principal, conforme o passo (`actions` do modelo).
function footer() {
  const { t } = screen.ctx;
  const { skip, back, primary } = actions(screen.state);
  const button = (type, label, cls) => el('button', { class: cls, onclick: () => dispatch({ type }) }, label);
  return el(
    'div',
    { class: 'actions' },
    skip ? button('skip', t('onboarding-skip'), 'text skip') : [],
    back ? button('back', t('onboarding-back'), 'tonal back') : [],
    primary ? button('continue', t(PRIMARY[primary]), 'filled continue') : [],
  );
}

const title = (text) => el('h1', { class: 'step-title' }, text);

// O conteúdo de cada passo: `(ctx, state) => [nós]`.
const STEPS = {
  'sign-in'({ api, t }, state) {
    const error = el('p', { class: 'step-error', role: 'alert' }, state.error ? t(...state.error) : []);
    const field = (id, key, label, type) => {
      const input = el('input', { id, type, value: state[key], autocomplete: 'off', spellcheck: 'false' });
      input.addEventListener('input', () => {
        dispatch({ type: 'edit', field: key, value: input.value });
        error.replaceChildren();
      });
      input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') dispatch({ type: 'continue' });
      });
      return el('label', { class: 'field' }, el('span', {}, label), input);
    };
    return [
      title(t('onboarding-sign-in-title')),
      el('p', {}, t('onboarding-sign-in-text')),
      el('button', { class: 'text', onclick: () => api.openLink('client-id-guide') }, t('onboarding-client-guide')),
      field('client-id', 'clientId', t('onboarding-client-id'), 'text'),
      field('client-secret', 'secret', t('onboarding-client-secret'), 'password'),
      el('p', { class: 'meta' }, t('onboarding-client-note')),
      error,
    ];
  },

  waiting({ api, t }, state) {
    const generate = el('button', {
      class: 'tonal generate-link',
      disabled: !state.url,
      onclick: () => {
        attempt(api.copy(state.url), () => toast(t('onboarding-link-copied')));
        dispatch({ type: 'show-link' });
      },
    }, t('onboarding-generate-link'));
    return [
      title(t('onboarding-waiting-title')),
      el('p', {}, t('onboarding-waiting-text')),
      el('p', {}, t('onboarding-waiting-other')),
      generate,
      state.linkShown
        ? el('input', { class: 'login-url', readonly: true, value: state.url, 'aria-label': t('onboarding-generate-link') })
        : [],
    ];
  },

  intro({ t }) {
    const block = (name) => card(t(`onboarding-intro-${name}-title`), el('p', {}, t(`onboarding-intro-${name}-text`)));
    return [title(t('onboarding-intro-title')), el('div', { class: 'blocks' }, ['stream', 'sync', 'offline'].map(block))];
  },

  files({ api, t }, state) {
    return [
      title(t('onboarding-files-title')),
      el('p', {}, t('onboarding-files-text', { path: state.mount })),
      el('button', { class: 'tonal', onclick: () => attempt(api.openMount()) }, t('onboarding-open-folder')),
    ];
  },

  offline({ t }) {
    return [
      title(t('onboarding-offline-title')),
      el('p', {}, t('onboarding-offline-text')),
      el('pre', { class: 'command' }, t('onboarding-offline-command')),
      el('p', {}, t('onboarding-offline-list')),
    ];
  },
};
