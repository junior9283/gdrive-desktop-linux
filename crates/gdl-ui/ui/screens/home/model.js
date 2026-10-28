// A lógica da janela principal sem DOM: o cabeçalho a partir do `Status`, a paginação e a
// ordenação da tabela de atividade. Os textos saem como `[chave, argumentos]` do `ui.ftl`.

import { formatAgo, formatDuration, formatSize } from '../../format.js';

const TITLES = {
  'signed-out': 'header-signed-out',
  starting: 'header-starting',
  synced: 'header-synced',
  error: 'header-error',
  syncing: 'header-syncing',
  offline: 'header-offline',
  'quota-full': 'header-quota-full',
  'needs-sign-in': 'header-needs-sign-in',
  paused: 'header-paused',
};

// "Última sincronização há…": menos de um minuto tem texto próprio.
function lastSync(status, now, locale) {
  if (!status.last_sync_ms) return ['header-never-synced'];
  if (now - status.last_sync_ms < 60_000) return ['header-last-sync-now'];
  return ['header-last-sync', { ago: formatAgo(status.last_sync_ms, now, locale) }];
}

// O login da conta venceu (só entrando de novo).
export const needsSignIn = (state) => state === 'needs-sign-in';

// O `Status` da conta mostrada: sem conta pedida o serviço manda o pior estado entre as contas, e
// o cabeçalho mostra o da própria conta.
export function shownStatus(status) {
  const own = status.accounts.find((a) => a.email === status.account);
  return own ? { ...status, state: own.state } : status;
}

// O que o menu da conta lista: as outras contas (a mostrada tem o cartão) e se cabe mais uma.
export function accountMenuFor(status) {
  return {
    others: status.accounts
      .filter((a) => a.email !== status.account)
      .map((a) => ({ email: a.email, needsSignIn: needsSignIn(a.state) })),
    shownNeedsSignIn: needsSignIn(shownStatus(status).state),
    canAdd: status.can_add_account,
  };
}

// Ainda sem conta de pé: nenhuma conectada, ou o serviço reconectando as guardadas.
export const withoutAccount = (state) => state === 'signed-out' || state === 'starting';

// `{ title, subtitle, pause, signIn }` do cabeçalho; `pause` é a chave do botão (ou `null` sem
// conta ou com a conta precisando entrar de novo, caso em que `signIn` pede o botão de entrar).
export function headerFor(status, now, locale) {
  const title = [TITLES[status.state] ?? 'header-synced', { count: status.syncing }];
  if (withoutAccount(status.state)) return { title, subtitle: null, pause: null, signIn: false };
  let subtitle;
  if (status.state === 'syncing' && status.remaining_secs >= 0) {
    subtitle = ['header-remaining', { time: formatDuration(status.remaining_secs, locale) }];
  } else if (status.state === 'error') {
    subtitle = ['header-error-detail', { count: status.errors }];
  } else if (status.state === 'offline') {
    subtitle = ['header-offline-detail'];
  } else if (status.state === 'quota-full') {
    subtitle = ['header-quota-detail'];
  } else {
    subtitle = lastSync(status, now, locale);
  }
  const signIn = needsSignIn(status.state);
  const pause = signIn ? null : status.paused ? 'action-resume' : 'action-pause';
  return { title, subtitle, pause, signIn };
}

// O texto do cartão "Drives compartilhados": quantos são e, se houver, quantos estão desligados
// (fora da montagem).
export function drivesCard(drives) {
  if (!drives.length) return ['card-drives-none'];
  const off = drives.filter((d) => !d.enabled).length;
  return off ? ['card-drives-off', { count: drives.length, off }] : ['card-drives-text', { count: drives.length }];
}

// Os envios que ainda dá para cancelar.
const CANCELABLE = new Set(['upload-queued', 'upload-paused', 'uploading']);

// O que o menu de uma linha da atividade permite: abrir (pasta, web, link) com o ID do item,
// Compartilhar só depois do envio e cancelar o envio que ainda não terminou.
export function rowActions(row) {
  const open = Boolean(row.item_id);
  return { open, share: open && row.state === 'uploaded', cancel: open && CANCELABLE.has(row.state) };
}

// "N itens off-line (tamanho)" do `OfflineSummary`.
export function offlineText(summary, locale) {
  return ['card-offline-text', { count: summary.pinned_count, size: formatSize(summary.pinned_bytes, locale) }];
}

// A página `page` (a partir de 1, limitada às que existem) com `size` linhas, e os números de
// "Mostrando X–Y de Z".
export function paginate(rows, page, size) {
  const total = rows.length;
  const pages = Math.max(1, Math.ceil(total / size));
  const current = Math.min(Math.max(1, page), pages);
  const start = (current - 1) * size;
  const slice = rows.slice(start, start + size);
  return {
    rows: slice,
    page: current,
    pages,
    from: slice.length ? start + 1 : 0,
    to: start + slice.length,
    total,
  };
}

const FIELDS = {
  name: (a, b) => a.name.localeCompare(b.name),
  size: (a, b) => a.size - b.size,
  time: (a, b) => a.time_ms - b.time_ms,
};

// Uma cópia ordenada por `key` (`name`, `size` ou `time`); empates ficam na ordem de antes.
export function sortRows(rows, key, dir) {
  const compare = FIELDS[key];
  const sign = dir === 'desc' ? -1 : 1;
  return [...rows].sort((a, b) => sign * compare(a, b));
}

// `home:<ação>[:<resto>]`: separa a ação do que ela recebe (a conta, por exemplo). Sem
// argumento (`null`), nenhuma ação.
export function splitArg(arg) {
  arg ??= '';
  const at = arg.indexOf(':');
  return at < 0 ? [arg, ''] : [arg.slice(0, at), arg.slice(at + 1)];
}
