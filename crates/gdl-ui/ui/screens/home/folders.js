// Pastas conectadas: as pastas do computador sincronizadas, de todas as contas, com "Abrir pasta"
// e "Parar de sincronizar"; "Adicionar pasta" leva à aba "Meu computador" das Preferências.

import { el } from '../../components.js';
import { folderItems } from '../folder-actions.js';

export async function render(ctx) {
  const { api, t, home } = ctx;
  const folders = await api.call('local_folders');
  return el('div', {},
    el('ul', { class: 'local-folders' }, folderItems(ctx, folders, home.status.accounts.length > 1)),
    el('button', { class: 'filled', onclick: () => api.openScreen('preferences', 'computer') }, t('pref-add-folder')));
}
