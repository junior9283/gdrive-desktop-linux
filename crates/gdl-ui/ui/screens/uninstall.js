// Desinstalar: o que vai acontecer, a escolha de apagar também os dados deste computador e o
// andamento. Com os dados e uma conta conectada, primeiro a saída da conta (as mesmas escolhas
// de "Desconectar conta" para os envios pendentes); depois a CLI tira os programas.

import { el, errorText, stylesheet } from '../components.js';
import { askLogout } from './home/disconnect.js';

const logo = () => el('img', { class: 'uninstall-logo', src: 'icons/app.png', alt: '' });

export async function render(root, ctx) {
  const { api, t } = ctx;
  await stylesheet('screens/uninstall.css');
  const purge = el('input', { type: 'checkbox', name: 'purge' });
  const progress = el('p', { class: 'uninstall-progress', role: 'status' });
  const cancel = el('button', { class: 'text', onclick: () => api.closeWindow() }, t('action-cancel'));
  const confirm = el('button', { class: 'filled danger', 'data-action': 'uninstall' }, t('uninstall-confirm'));
  const main = el(
    'main',
    { class: 'uninstall' },
    logo(),
    el('h1', {}, t('uninstall-title')),
    el('p', {}, t('uninstall-body')),
    el('label', { class: 'uninstall-purge' }, purge,
      el('span', {}, el('strong', {}, t('uninstall-purge')), el('br'), t('uninstall-purge-hint'))),
    progress,
    el('div', { class: 'actions' }, cancel, confirm),
  );
  root.replaceChildren(main);

  const busy = (on, text = '') => {
    for (const control of [purge, cancel, confirm]) control.disabled = on;
    progress.textContent = text;
  };
  confirm.addEventListener('click', async () => {
    busy(true);
    try {
      if (purge.checked && !(await signOut(ctx, (text) => busy(true, text)))) {
        busy(false);
        return;
      }
      busy(true, t('uninstall-progress'));
      await api.uninstall(purge.checked);
      main.replaceChildren(
        logo(),
        el('h1', {}, t('uninstall-done')),
        el('p', {}, t('uninstall-done-body')),
        el('div', { class: 'actions' },
          el('button', { class: 'filled', onclick: () => api.closeWindow() }, t('action-close'))),
      );
    } catch (error) {
      busy(false, errorText(error));
    }
  });
}

// Desconecta a conta, se houver uma; `false` se o usuário desistiu.
async function signOut(ctx, say) {
  const { api, t } = ctx;
  const status = await api.call('status');
  if (!status.account) return true;
  const choice = await askLogout(ctx, status);
  if (!choice) return false;
  say(t('disconnect-progress'));
  // Com "terminar os envios", a conta só sai quando eles acabarem: o serviço avisa a cada mudança.
  const signedOut = new Promise((resolve) => {
    const check = async () => {
      if (!(await api.call('status')).account) resolve();
    };
    api.on('status-changed', () => check().catch(() => {})).then(check).catch(() => {});
  });
  await api.call('logout', choice);
  say(t('uninstall-waiting'));
  await signedOut;
  return true;
}
