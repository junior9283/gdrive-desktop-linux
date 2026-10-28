// Notificações: o histórico dos avisos, marcados como lidos ao abrir, com a ação de cada um.

import { attempt, el, icon, toast, toastError } from '../../components.js';
import { formatAgo } from '../../format.js';
import { notificationActions } from './model.js';

export async function render({ api, t, locale, home }) {
  const [items, settings] = await Promise.all([api.call('notifications'), api.call('notification_settings')]);
  if (items.some((n) => !n.read)) api.call('mark_notifications_read').catch(() => {});
  // Abrir a visão marca tudo como lido: o selo zera sem perguntar de novo ao serviço.
  home.setUnread(0);
  if (!items.length) return el('p', { class: 'empty' }, t('notifications-empty'));
  const mutable = new Set(settings.filter((s) => s.enabled).map((s) => s.kind));
  const now = Date.now();
  const button = (n, key) => key === 'mute'
    ? el('button', { class: 'text mute', onclick: () =>
        attempt(api.call('set_notification_enabled', { kind: n.kind, enabled: false }), () => toast(t('notification-muted'))) },
      t('notification-mute'))
    : el('button', { class: 'text', onclick: () =>
        api.call('invoke_notification_action', { id: n.id, key }).catch(toastError) }, n.action_label);
  return el(
    'ul',
    { class: 'list' },
    items.map((n) =>
      el(
        'li',
        { class: n.read ? '' : 'unread' },
        el('div', {},
          el('span', { class: 'name' }, n.title),
          el('span', { class: 'body' }, n.body),
          el('span', { class: 'meta' }, formatAgo(n.time_ms, now, locale))),
        el('div', { class: 'row-actions' },
          notificationActions(n, mutable).map((key) => button(n, key)),
          el('button', { class: 'icon-button', 'aria-label': t('notification-dismiss'), onclick: () =>
            api.call('dismiss_notification', { id: n.id }).catch(toastError) }, icon('close'))),
      ),
    ),
  );
}
