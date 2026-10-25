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

export async function disconnect(ctx) {
  const { api, t } = ctx;
  const status = await api.call('status');
  const count = status.pending_uploads;
  const warning = el('p', {}, t('disconnect-offline-warning'));
  const choices = count === 0
    ? { body: [el('p', {}, t('disconnect-body', { account: status.account }))], value: () => ({ mode: '', dir: '' }) }
    : pendingChoices(ctx, count);
  const ok = await confirmDialog({
    title: t('disconnect-title'),
    body: [...choices.body, warning],
    confirm: t('account-disconnect'),
    cancel: t('action-cancel'),
    danger: true,
    ready: choices.ready,
  });
  if (!ok) return;
  toast(t('disconnect-progress'));
  attempt(api.call('logout', choices.value()), () => toast(t('disconnect-done')));
}
