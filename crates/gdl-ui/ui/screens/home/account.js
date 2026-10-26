// Menu da conta: foto, nome, e-mail, uso do armazenamento e as ações da conta.

import { el, popupMenu } from '../../components.js';
import { formatSize } from '../../format.js';
import { disconnect } from './disconnect.js';

// `account_info` vai ao Drive: o resultado (com a foto já convertida) vale por um minuto, e quem
// pede durante a busca (o botão e o menu, na abertura) recebe a mesma.
const FRESH_MS = 60_000;
let cached = null;

// A foto (bytes vindos do serviço) como endereço `data:`.
function photoUrl(bytes) {
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return `data:image/png;base64,${btoa(binary)}`;
}

function accountInfo(api, account) {
  if (cached?.account === account && Date.now() - cached.at < FRESH_MS) return cached.promise;
  const promise = api.call('account_info').then((info) => ({
    info,
    photo: info.photo_png.length ? photoUrl(info.photo_png) : null,
  }));
  cached = { at: Date.now(), account, promise };
  // Uma falha não fica guardada: o próximo pedido tenta de novo.
  promise.catch(() => {
    if (cached?.promise === promise) cached = null;
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

// O menu abre na hora com o e-mail; nome, foto e uso chegam depois.
export function accountMenu(anchor, ctx) {
  const { api, t, locale, home } = ctx;
  const account = home.status?.account ?? '';
  const card = el('span', { class: 'account-card' }, el('strong', {}, account));
  popupMenu(anchor, [
    { node: card },
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
