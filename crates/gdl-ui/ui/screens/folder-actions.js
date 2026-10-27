// As ações de uma pasta do computador sincronizada, as mesmas na aba "Meu computador" e na visão
// "Pastas conectadas": a linha com o aviso do estado e o menu "Abrir pasta", "Escolher novo local"
// (pasta parada) e "Parar de sincronizar" (com confirmação).

import { attempt, confirmDialog, el, errorText, menuButton, toast } from '../components.js';
import { folderRows, refusalKey } from './preferences/folders.js';

// Os itens da lista `ul.local-folders` das pastas do serviço (`LocalFolders`), ou o aviso de
// lista vazia.
export function folderItems(ctx, folders, manyAccounts) {
  const rows = folderRows(folders, manyAccounts);
  return rows.length
    ? rows.map((row) => folderRow(ctx, row))
    : [el('li', { class: 'empty' }, ctx.t('pref-computer-empty'))];
}

// A linha de `row` (de `folderRows`) como `li.local-folder`.
export function folderRow({ api, t }, row) {
  const args = { account: row.account, id: row.id };
  const confirmDeletes = row.needsConfirm
    ? el('button', { class: 'text', onclick: () => attempt(api.call('confirm_folder_deletes', args)) }, t('folder-confirm-deletes'))
    : [];
  return el('li', { class: 'local-folder', 'data-path': row.path },
    el('div', { class: 'field' },
      el('strong', {}, row.name),
      el('span', { class: 'path' }, row.path),
      row.accountLabel ? el('span', { class: 'meta' }, row.accountLabel) : [],
      row.notice ? el('span', { class: 'folder-notice' }, t(...row.notice), confirmDeletes) : []),
    menuButton(t('row-menu'), () => [
      { id: 'open', label: t('folder-open'), action: () => attempt(api.openLocalFolder(row.account, row.id)) },
      ...(row.canRelocate ? [{ id: 'relocate', label: t('folder-relocate'), action: () => relocate({ api, t }, row) }] : []),
      { id: 'stop', label: t('folder-stop'), action: () => stopFolder({ api, t }, row) },
    ]));
}

// A pasta mudou de lugar: o seletor do sistema escolhe onde ela está agora.
async function relocate({ api, t }, row) {
  const path = await api.pickFolder(t('folder-relocate')).catch(() => null);
  if (!path) return;
  api.call('relocate_local_folder', { account: row.account, id: row.id, path })
    .then(() => toast(t('folder-added', { name: row.name })))
    .catch(refusalToast(t));
}

// O aviso de um erro ao adicionar ou mudar uma pasta: a recusa do serviço com texto próprio.
export function refusalToast(t) {
  return (error) => {
    const key = error?.name === 'Refused' && refusalKey(error.message);
    toast(key ? t(key) : errorText(error));
  };
}

async function stopFolder({ api, t }, row) {
  const ok = await confirmDialog({
    title: t('folder-stop-title', { name: row.name }),
    body: t('folder-stop-body'),
    confirm: t('folder-stop'),
    cancel: t('action-cancel'),
    danger: true,
  });
  if (!ok) return;
  attempt(api.call('remove_local_folder', { account: row.account, id: row.id }), () =>
    toast(t('folder-stopped', { name: row.name })));
}
