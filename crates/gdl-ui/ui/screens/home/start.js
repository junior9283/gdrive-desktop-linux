// Início: cartões do que precisa de atenção, dos arquivos off-line, dos compartilhados, do estado
// da sincronização e do Gemini (na web).

import { el } from '../../components.js';
import { headerFor, offlineText } from './model.js';

// Os números do off-line e dos compartilhados (`shared_with_me` vai ao Drive) valem por um
// minuto: o Início redesenha a cada mudança do estado.
const FRESH_MS = 60_000;
let cached = null;

function extras(api) {
  if (!cached || Date.now() - cached.at > FRESH_MS) {
    const data = Promise.all([api.call('offline_summary'), api.call('shared_with_me')]);
    cached = { at: Date.now(), data };
    // Um erro não fica guardado.
    data.catch(() => {
      if (cached?.data === data) cached = null;
    });
  }
  return cached.data;
}

function card(title, text, label, onclick) {
  return el(
    'section',
    { class: 'card' },
    el('h2', {}, title),
    text,
    label ? el('button', { class: 'text', onclick }, label) : [],
  );
}

function attention({ status, t, api }) {
  const title = t('card-attention');
  if (status.state === 'quota-full') {
    return card(title, el('p', {}, t('header-quota-detail')), t('storage-manage'), () => api.openLink('storage'));
  }
  if (status.errors > 0) {
    return card(title, el('p', {}, t('card-attention-errors', { count: status.errors })),
      t('card-attention-open'), () => api.openScreen('errors'));
  }
  return card(title, el('p', {}, t('card-attention-none')));
}

// Os cartões aparecem na hora com o `Status`; off-line e compartilhados chegam depois.
export async function render({ api, t, locale, home }) {
  const status = home.status;
  const offlineCount = el('p');
  const sharedCount = el('p');
  extras(api)
    .then(([summary, sharedItems]) => {
      offlineCount.textContent = t(...offlineText(summary, locale));
      sharedCount.textContent = t('card-shared-text', { count: sharedItems.length });
    })
    .catch(() => {});
  return el(
    'div',
    { class: 'cards' },
    attention({ status, t, api }),
    card(t('card-offline'), offlineCount, t('card-offline-open'), () => home.show('offline')),
    card(t('card-shared'), sharedCount, t('card-shared-open'), () => home.show('shared')),
    card(t('card-sync'), el('p', {}, t(...headerFor(status, Date.now(), locale).title)),
      t('card-sync-open'), () => home.show('activity')),
    card(t('card-gemini'), el('p', {}, t('card-gemini-text')), t('card-gemini-open'), () => api.openLink('gemini')),
  );
}
