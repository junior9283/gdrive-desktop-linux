// Menu "Mais opções".

import { attempt, confirmDialog, popupMenu, toast } from '../../components.js';
import { disconnect, disconnectAll } from './disconnect.js';
import { withoutAccount } from './model.js';

export function moreMenu(anchor, ctx) {
  const { api, t, home } = ctx;
  const errors = home.status?.errors ?? 0;
  const signedIn = home.status && !withoutAccount(home.status.state);
  popupMenu(anchor, [
    { id: 'offline', label: t('view-offline'), action: () => api.openScreen('offline'), disabled: !signedIn },
    { id: 'errors', label: t('more-errors', { count: errors }), action: () => api.openScreen('errors'), disabled: !signedIn },
    { id: 'notification-settings', label: t('more-notification-settings'), action: () => api.openScreen('notification-settings') },
    { id: 'reset', label: t('more-reset'), action: () => reset(ctx) },
    { id: 'disconnect', label: t('account-disconnect'), action: () => disconnect(ctx), disabled: !signedIn },
    { id: 'disconnect-all', label: t('account-disconnect-all'), action: () => disconnectAll(ctx), disabled: !signedIn },
    '-',
    { id: 'preferences', label: t('more-preferences'), action: () => api.openScreen('preferences') },
    { id: 'about', label: t('more-about'), action: () => api.openScreen('about') },
    { id: 'quit', label: t('more-quit'), action: () => quit(ctx) },
  ]);
}

async function reset({ api, t }) {
  const ok = await confirmDialog({
    title: t('reset-title'),
    body: t('reset-body'),
    confirm: t('reset-confirm'),
    cancel: t('action-cancel'),
  });
  if (ok) attempt(api.call('reset_preferences'), () => toast(t('reset-done')));
}

// Com itens sincronizando, pergunta antes de encerrar o serviço.
async function quit({ api, t, home }) {
  const pending = Math.max(home.status?.syncing ?? 0, home.status?.pending_uploads ?? 0);
  if (pending > 0) {
    const ok = await confirmDialog({
      title: t('quit-title'),
      body: t('quit-body', { count: pending }),
      confirm: t('more-quit'),
      cancel: t('action-cancel'),
    });
    if (!ok) return;
  }
  await api.call('quit').catch(() => {});
  api.closeWindow();
}
