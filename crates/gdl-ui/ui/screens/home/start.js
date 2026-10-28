// Início: cartões do que precisa de atenção, dos arquivos off-line, dos compartilhados, do estado
// da sincronização e do Gemini (na web).

import { card, el } from '../../components.js';
import { foldersCard } from '../preferences/folders.js';
import { drivesCard, headerFor, offlineText } from './model.js';
import { currentAccount } from '../../state.js';

// Os números do off-line, dos compartilhados e dos drives (`shared_with_me` e `shared_drives` vão ao
// Drive) valem por um minuto: o Início redesenha a cada mudança do estado.
const FRESH_MS = 60_000;
let cached = null;

// Guardados por conta: trocar de conta lê de novo.
function extras(api) {
  const account = currentAccount();
  if (!cached || cached.account !== account || Date.now() - cached.at > FRESH_MS) {
    const data = Promise.all([api.call('offline_summary'), api.call('shared_with_me'), api.call('shared_drives')]);
    cached = { account, at: Date.now(), data };
    // Um erro não fica guardado.
    data.catch(() => {
      if (cached?.data === data) cached = null;
    });
  }
  return cached.data;
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

// Os cartões aparecem na hora com o `Status`; off-line, compartilhados e drives chegam depois.
export async function render({ api, t, locale, home }) {
  const status = home.status;
  const offlineCount = el('p');
  const sharedCount = el('p');
  const foldersCount = el('p');
  const drivesCount = el('p');
  api.call('local_folders')
    .then((list) => {
      foldersCount.textContent = t(...foldersCard(list));
    })
    .catch(() => {});
  extras(api)
    .then(([summary, sharedItems, drives]) => {
      offlineCount.textContent = t(...offlineText(summary, locale));
      sharedCount.textContent = t('card-shared-text', { count: sharedItems.length });
      drivesCount.textContent = t(...drivesCard(drives));
    })
    .catch(() => {});
  return el(
    'div',
    { class: 'cards' },
    attention({ status, t, api }),
    card(t('card-offline'), offlineCount, t('card-offline-open'), () => home.show('offline')),
    card(t('card-folders'), foldersCount, t('card-folders-open'), () => home.show('folders')),
    card(t('card-shared'), sharedCount, t('card-shared-open'), () => home.show('shared')),
    card(t('card-drives'), drivesCount, t('card-drives-open'), () => home.show('drives')),
    card(t('card-sync'), el('p', {}, t(...headerFor(status, Date.now(), locale).title)),
      t('card-sync-open'), () => home.show('activity')),
    card(t('card-gemini'), el('p', {}, t('card-gemini-text')), t('card-gemini-open'), () => api.openLink('gemini')),
  );
}
