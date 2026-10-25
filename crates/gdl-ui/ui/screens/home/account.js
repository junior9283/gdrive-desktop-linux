// Menu da conta: foto, nome, e-mail, uso do armazenamento e as ações da conta.

import { el, popupMenu } from '../../components.js';
import { formatSize } from '../../format.js';
import { disconnect } from './disconnect.js';

// `account_info` vai ao Drive: o resultado (com a foto já convertida) vale por um minuto.
const FRESH_MS = 60_000;
let cached = null;

// A foto (bytes vindos do serviço) como endereço `data:`.
function photoUrl(bytes) {
  let binary = '';
  for (const b of bytes) binary += String.fromCharCode(b);
  return `data:image/png;base64,${btoa(binary)}`;
}

async function accountInfo(api, account) {
  if (cached?.info.email === account && Date.now() - cached.at < FRESH_MS) return cached;
  const info = await api.call('account_info');
  const photo = info.photo_png.length ? photoUrl(info.photo_png) : null;
  cached = { at: Date.now(), info, photo };
  return cached;
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
        ...(photo ? [el('img', { class: 'avatar', src: photo, alt: '' })] : []),
        el('span', {}, el('strong', {}, info.name || info.email), el('br'), info.email, el('br'), usage),
      );
    })
    // Sem os dados da conta, o cartão fica só com o e-mail.
    .catch(() => {});
}
