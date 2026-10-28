// Menu "Mais opções".

import { attempt, confirmDialog, popupMenu, toast } from '../../components.js';
import { disconnect, disconnectAll } from './disconnect.js';
import { withoutAccount } from './model.js';
import { currentAccount } from '../../state.js';

export async function moreMenu(anchor, ctx) {
  const { api, t, home } = ctx;
  const errors = home.status?.errors ?? 0;
  const signedIn = home.status && !withoutAccount(home.status.state);
  const verbose = await api.call('verbose_logging').catch(() => false);
  popupMenu(anchor, [
    { id: 'offline', label: t('view-offline'), action: () => api.openScreen('offline'), disabled: !signedIn },
    { id: 'errors', label: t('more-errors', { count: errors }), action: () => api.openScreen('errors'), disabled: !signedIn },
    { id: 'notification-settings', label: t('more-notification-settings'), action: () => api.openScreen('notification-settings') },
    { id: 'shared-drives', label: t('more-shared-drives'), action: () => api.openScreen('shared-drives', currentAccount()), disabled: !signedIn },
    '-',
    { id: 'feedback', label: t('more-feedback'), action: () => api.openScreen('feedback') },
    { id: 'diagnostics', label: t('more-diagnostics'), action: () => attempt(api.call('generate_diagnostics'), () => toast(t('diagnostics-done'))) },
    {
      id: 'verbose',
      label: t(verbose ? 'more-verbose-off' : 'more-verbose-on'),
      action: () => attempt(api.call('set_verbose_logging', { on: !verbose }), () => toast(t(verbose ? 'verbose-off-done' : 'verbose-on-done'))),
    },
    '-',
    { id: 'reset', label: t('more-reset'), action: () => reset(ctx) },
    { id: 'reset-local', label: t('more-reset-local'), action: () => resetLocal(ctx), disabled: !signedIn },
    { id: 'disconnect', label: t('account-disconnect'), action: () => disconnect(ctx), disabled: !signedIn },
    { id: 'disconnect-all', label: t('account-disconnect-all'), action: () => disconnectAll(ctx), disabled: !signedIn },
    '-',
    { id: 'preferences', label: t('more-preferences'), action: () => api.openScreen('preferences') },
    { id: 'about', label: t('more-about'), action: () => api.openScreen('about') },
    { id: 'quit', label: t('more-quit'), action: () => quit(ctx) },
  ]);
}

// O que a bandeja pede ao abrir a janela (`home:<argumento>`): ela não tem diálogo, então a
// confirmação acontece aqui.
const ARG_ACTIONS = { 'reset-preferences': reset, 'disconnect-all': disconnectAll };

export function runArg(arg, ctx) {
  ARG_ACTIONS[arg]?.(ctx);
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

// Apaga banco e cache da conta e a reindexa; com envios pendentes o serviço recusa e o erro diz
// quantos.
async function resetLocal({ api, t }) {
  const ok = await confirmDialog({
    title: t('reset-local-title'),
    body: t('reset-local-body'),
    confirm: t('reset-confirm'),
    cancel: t('action-cancel'),
    danger: true,
  });
  if (ok) attempt(api.call('reset_local_data'), () => toast(t('reset-local-done')));
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
