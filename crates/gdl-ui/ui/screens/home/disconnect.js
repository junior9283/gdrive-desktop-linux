// Desconectar a conta: sem pendências, uma confirmação; com arquivos não sincronizados, as três
// escolhas (terminar e sair, mover para uma pasta, excluir com confirmação explícita).

import { attempt, choice, confirmDialog, el, toast } from '../../components.js';

// As três escolhas com pendências: `value()` dá `{ mode, dir }` e `ready()` diz se a escolha
// está completa (a pasta escolhida, a exclusão confirmada).
function pendingChoices({ api, t }, count) {
  let dir = '';
  const folder = el('span', { class: 'meta' });
  const sure = el('input', { type: 'checkbox' });
  const radio = (value) => el('input', { type: 'radio', name: 'mode', value, checked: value === 'finish' });
  const move = radio('move');
  move.addEventListener('change', async () => {
    dir = (await api.pickFolder(t('disconnect-pick-folder')).catch(() => null)) ?? '';
    folder.textContent = dir ? ` (${dir})` : '';
    // A pasta chega depois do `change`: avisa o diálogo de novo.
    move.dispatchEvent(new Event('input', { bubbles: true }));
  });
  const options = el('div', { class: 'choices' },
    choice(radio('finish'), t('disconnect-finish')),
    choice(move, t('disconnect-move', { count }), folder),
    choice(radio('discard'), t('disconnect-discard', { count }),
      el('label', { class: 'sure' }, sure, t('disconnect-discard-sure'))));
  const mode = () => options.querySelector('input[name=mode]:checked').value;
  return {
    body: [el('p', {}, t('disconnect-pending', { count })), options],
    value: () => ({ mode: mode(), dir: mode() === 'move' ? dir : '' }),
    ready: () => (mode() !== 'move' || dir !== '') && (mode() !== 'discard' || sure.checked),
  };
}

// Pergunta como desconectar as contas `emails`, com `pending` envios não sincronizados no total:
// `{ mode, dir }` para o `logout`, ou `null` se o usuário desistiu (também usada pela
// desinstalação).
export async function askLogout(ctx, { emails, pending }) {
  const { t } = ctx;
  const one = emails.length === 1;
  const warning = el('p', {}, t('disconnect-offline-warning'));
  const intro = one ? t('disconnect-body', { account: emails[0] }) : t('disconnect-all-body', { count: emails.length });
  const choices = pending === 0
    ? { body: [el('p', {}, intro)], value: () => ({ mode: '', dir: '' }) }
    : pendingChoices(ctx, pending);
  const ok = await confirmDialog({
    title: t(one ? 'disconnect-title' : 'disconnect-all-title'),
    body: [...choices.body, warning],
    confirm: t(one ? 'account-disconnect' : 'account-disconnect-all'),
    cancel: t('action-cancel'),
    danger: true,
    ready: choices.ready,
  });
  return ok ? choices.value() : null;
}

// Os e-mails de todas as contas e os envios pendentes delas somados (o `Status` já traz os dois).
export async function allAccounts(api) {
  const { accounts } = await api.call('status', { account: '' });
  return {
    emails: accounts.map((a) => a.email),
    pending: accounts.reduce((sum, a) => sum + a.pending_uploads, 0),
  };
}

export async function disconnect(ctx) {
  const { api, t } = ctx;
  const status = await api.call('status');
  const choice = await askLogout(ctx, { emails: [status.account], pending: status.pending_uploads });
  if (!choice) return;
  toast(t('disconnect-progress'));
  attempt(api.call('logout', choice), () => toast(t('disconnect-done')));
}

export async function disconnectAll(ctx) {
  const { api, t } = ctx;
  const choice = await askLogout(ctx, await allAccounts(api));
  if (!choice) return;
  toast(t('disconnect-progress'));
  attempt(api.call('logout_all', choice), () => toast(t('disconnect-all-done')));
}
