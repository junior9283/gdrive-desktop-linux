// Notificações: o histórico dos avisos, marcados como lidos ao abrir, com a ação de cada um.

import { el, icon, toastError } from '../../components.js';
import { formatAgo } from '../../format.js';

export async function render({ api, t, locale, home }) {
  const items = await api.call('notifications');
  if (items.some((n) => !n.read)) api.call('mark_notifications_read').catch(() => {});
  // Abrir a visão marca tudo como lido: o selo zera sem perguntar de novo ao serviço.
  home.setUnread(0);
  if (!items.length) return el('p', { class: 'empty' }, t('notifications-empty'));
  const now = Date.now();
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
          n.action_key
            ? el('button', { class: 'text', onclick: () =>
                api.call('invoke_notification_action', { id: n.id, key: n.action_key }).catch(toastError) }, n.action_label)
            : [],
          el('button', { class: 'icon-button', 'aria-label': t('notification-dismiss'), onclick: () =>
            api.call('dismiss_notification', { id: n.id }).catch(toastError) }, icon('close'))),
      ),
    ),
  );
}
