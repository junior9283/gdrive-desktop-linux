// Pastas conectadas: as pastas do computador sincronizadas, de todas as contas, com "Abrir pasta"
// e "Parar de sincronizar"; "Adicionar pasta" leva à aba "Meu computador" das Preferências.

import { attempt, el, toast } from '../../components.js';
import { folderItems } from '../folder-actions.js';
import { folderName } from '../preferences/folders.js';

export async function render(ctx) {
  const { api, t, home } = ctx;
  const [folders, prefs] = await Promise.all([
    api.call('local_folders'),
    api.call('preferences').catch(() => []),
  ]);
  const excludedPref = prefs.find((p) => p.key === 'ExcludedFolders');
  const excluded = excludedPref?.value ? JSON.parse(excludedPref.value) : [];
  const managed = Boolean(excludedPref?.managed);
  const busy = home.status?.state === 'checking' || home.status?.state === 'syncing' || home.status?.paused;

  const saveExcluded = (next, msg) =>
    attempt(api.call('set_preferences', { values: [['ExcludedFolders', JSON.stringify(next)]] }), () => {
      toast(msg);
      home.refresh();
    });

  const pickExcluded = async () => {
    const dir = await api.pickFolder(t('pref-pick-folder')).catch(() => null);
    if (!dir || excluded.includes(dir)) return;
    saveExcluded([...excluded, dir], t('pref-excluded-added', { name: folderName(dir) }));
  };

  return el('div', {},
    el('ul', { class: 'local-folders' }, folderItems(ctx, folders, home.status.accounts.length > 1)),
    el('div', { class: 'actions-row', style: 'display: flex; gap: 8px; margin-top: 16px;' },
      el('button', { class: 'filled', onclick: () => api.openScreen('preferences', 'computer') }, t('pref-add-folder')),
      el('button', { class: 'button', disabled: busy, onclick: () => attempt(api.call('sync_now', { account: '', folder_id: 0 }), () => toast(t('sync-now-toast'))) }, t('button-sync-now')),
    ),
    el('h2', { style: 'margin-top: 24px;' }, t('pref-excluded-folders')),
    el('p', { class: 'meta' }, t('pref-excluded-folders-text')),
    el('ul', { class: 'local-folders excluded-folders' }, ...(excluded.length
      ? excluded.map((path) => el('li', { class: 'local-folder', 'data-path': path },
        el('div', { class: 'field' },
          el('strong', {}, folderName(path)),
          el('span', { class: 'path' }, path)),
        el('button', {
          class: 'text',
          disabled: managed,
          onclick: () => saveExcluded(excluded.filter((p) => p !== path), t('pref-excluded-removed', { name: folderName(path) })),
        }, t('pref-excluded-remove'))))
      : [el('li', { class: 'empty' }, t('pref-excluded-empty'))])),
    el('div', { class: 'actions-row', style: 'display: flex; gap: 8px; margin-top: 12px;' },
      el('button', { class: 'tonal add-excluded-folder', disabled: managed, onclick: pickExcluded }, t('pref-add-excluded-folder')),
    ),
  );
}
