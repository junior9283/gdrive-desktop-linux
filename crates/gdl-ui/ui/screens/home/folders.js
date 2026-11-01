// Pastas conectadas: as pastas do computador sincronizadas, de todas as contas, com "Abrir pasta"
// e "Parar de sincronizar"; "Adicionar pasta" leva à aba "Meu computador" das Preferências.

import { attempt, el, toast } from '../../components.js';
import { folderItems } from '../folder-actions.js';

export async function render(ctx) {
  const { api, t, home } = ctx;
  const folders = await api.call('local_folders');
  const busy = home.status?.state === 'checking' || home.status?.state === 'syncing' || home.status?.paused;
  return el('div', {},
    el('ul', { class: 'local-folders' }, folderItems(ctx, folders, home.status.accounts.length > 1)),
    el('div', { class: 'actions-row', style: 'display: flex; gap: 8px; margin-top: 16px;' },
      el('button', { class: 'filled', onclick: () => api.openScreen('preferences', 'computer') }, t('pref-add-folder')),
      el('button', { class: 'button', disabled: busy, onclick: () => attempt(api.call('sync_now', { account: '', folder_id: 0 }), () => toast(t('sync-now-toast'))) }, t('button-sync-now')),
    ),
  );
}
