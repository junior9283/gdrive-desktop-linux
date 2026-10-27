// Menu da conta: foto, nome, e-mail, uso do armazenamento, as outras contas e as ações da conta.

import { el, popupMenu } from '../../components.js';
import { formatSize } from '../../format.js';
import { setCurrentAccount } from '../../state.js';
import { disconnect } from './disconnect.js';
import { accountMenuFor } from './model.js';

// `account_info` vai ao Drive: o resultado (com a foto já convertida) vale por um minuto, e quem
// pede durante a busca (o botão e o menu, na abertura) recebe a mesma. Um por conta.
const FRESH_MS = 60_000;
const cached = new Map();

// A foto (bytes vindos do serviço) como endereço `data:`.
function photoUrl(bytes) {
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return `data:image/png;base64,${btoa(binary)}`;
}

function accountInfo(api, account) {
  const hit = cached.get(account);
  if (hit && Date.now() - hit.at < FRESH_MS) return hit.promise;
  const promise = api.call('account_info', { account }).then((info) => ({
    info,
    photo: info.photo_png.length ? photoUrl(info.photo_png) : null,
  }));
  cached.set(account, { at: Date.now(), promise });
  // Uma falha não fica guardada: o próximo pedido tenta de novo.
  promise.catch(() => {
    if (cached.get(account)?.promise === promise) cached.delete(account);
  });
  return promise;
}

const avatar = (photo) => el('img', { class: 'avatar', src: photo, alt: '' });

// O botão da conta: a inicial na hora e a foto quando chegar (só uma busca por conta).
export function accountButton(button, api, account = '') {
  if (button.dataset.account === account) return;
  button.dataset.account = account;
  button.textContent = (account || '?').slice(0, 1).toUpperCase();
  if (!account) return;
  accountInfo(api, account)
    .then(({ photo }) => {
      // A conta pode ter mudado enquanto a foto vinha.
      if (photo && button.dataset.account === account) button.replaceChildren(avatar(photo));
    })
    // Sem a foto, fica a inicial.
    .catch(() => {});
}

// Uma conta da lista do menu: a inicial na hora (e a foto quando chegar), o e-mail e, se o login
// venceu, o aviso.
function accountRow(api, t, { email, needsSignIn }) {
  const picture = el('span', { class: 'avatar initial' }, email.slice(0, 1).toUpperCase());
  accountInfo(api, email)
    .then(({ photo }) => photo && picture.replaceWith(avatar(photo)))
    // Sem a foto, fica a inicial.
    .catch(() => {});
  return el('span', { class: 'account-row' }, picture,
    el('span', {}, email, needsSignIn ? [el('br'), el('small', { class: 'needs-sign-in' }, t('account-needs-sign-in'))] : []));
}

// O menu abre na hora com o e-mail; nome, foto e uso chegam depois.
export function accountMenu(anchor, ctx) {
  const { api, t, locale, home } = ctx;
  const status = home.status ?? { account: '', accounts: [], can_add_account: false };
  const account = status.account;
  const menu = accountMenuFor(status);
  const card = el('span', { class: 'account-card' }, el('strong', {}, account));
  popupMenu(anchor, [
    { node: card },
    '-',
    ...menu.others.map((other) => ({
      id: `switch-${other.email}`,
      label: accountRow(api, t, other),
      action: () => setCurrentAccount(other.email),
    })),
    ...(menu.shownNeedsSignIn
      ? [{ id: 'sign-in-again', label: t('account-sign-in-again'), action: () => api.openScreen('onboarding', `resume:${account}`) }]
      : []),
    ...(menu.canAdd ? [{ id: 'add-account', label: t('account-add'), action: () => api.openScreen('onboarding', 'add') }] : []),
    '-',
    { id: 'storage', label: t('storage-manage'), action: () => api.openLink('storage') },
    { id: 'disconnect', label: t('account-disconnect'), action: () => disconnect(ctx) },
    '-',
    { id: 'privacy', label: t('link-privacy'), action: () => api.openLink('privacy') },
    { id: 'terms', label: t('link-terms'), action: () => api.openLink('terms') },
  ]);
  accountInfo(api, account)
    .then(({ info, photo }) => {
      const used = formatSize(info.quota_used, locale);
      const usage = info.quota_total < 0
        ? t('account-usage-unlimited', { used })
        : t('account-usage', { used, total: formatSize(info.quota_total, locale) });
      card.replaceChildren(
        ...(photo ? [avatar(photo)] : []),
        el('span', {}, el('strong', {}, info.name || info.email), el('br'), info.email, el('br'), usage),
      );
    })
    // Sem os dados da conta, o cartão fica só com o e-mail.
    .catch(() => {});
}
