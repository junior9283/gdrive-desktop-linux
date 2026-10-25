// Preferências de notificação: liga e desliga cada família de aviso opcional
// (`notification_settings`); a mudança vale na hora.

import { choice, el, stylesheet, toastError } from '../components.js';

export async function render(root, { api, t }) {
  const css = stylesheet('screens/notification-settings.css');
  const [settings] = await Promise.all([api.call('notification_settings'), css]);
  const toggle = (setting) => {
    const input = el('input', { type: 'checkbox', checked: setting.enabled, 'data-kind': setting.kind });
    input.addEventListener('change', () =>
      api.call('set_notification_enabled', { kind: setting.kind, enabled: input.checked }).catch((e) => {
        input.checked = !input.checked;
        toastError(e);
      }));
    return choice(input, setting.label);
  };
  root.replaceChildren(
    el('main', { class: 'notification-settings' },
      el('h1', {}, t('notification-settings-title')),
      el('p', { class: 'meta' }, t('notification-settings-text')),
      settings.length
        ? el('div', { class: 'choices' }, settings.map(toggle))
        : el('p', { class: 'empty' }, t('notification-settings-empty')),
      el('div', { class: 'actions' },
        el('button', { class: 'filled', onclick: () => api.closeWindow() }, t('action-close')))),
  );
}
