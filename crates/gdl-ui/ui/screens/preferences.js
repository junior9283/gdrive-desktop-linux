// Preferências: as abas "Meu computador", "Google Drive" e "Configurações", e o rodapé com
// "Alterações pendentes", Cancelar e Salvar (todas as mudanças num `set_preferences` só). Uma
// chave da política aparece bloqueada. A lógica fica em `preferences/model.js`.

import { attempt, choice, confirmDialog, el, saveFooter, stylesheet, toast, toastError } from '../components.js';
import { folderItems, refusalToast } from './folder-actions.js';
import { formatSize } from '../format.js';
import { screenArg } from '../router.js';
import { currentAccount } from '../state.js';
import { disconnect } from './home/disconnect.js';
import { configurator, folderArg, folderName } from './preferences/folders.js';
import {
  diff, fromPrefs, gbToKb, hotkeyFrom, kbToGb, numberOrNull, proxyMode, proxyPatch, validate,
} from './preferences/model.js';

// O atalho quando `SearchHotKey` não foi definido (o mesmo `DEFAULT_TRIGGER` da bandeja).
const DEFAULT_HOTKEY = 'Ctrl+Alt+G';
// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta).
const screen = { root: null, ctx: null, tab: 'drive', listening: null, listeningArg: null };
// O que veio do serviço e o que o usuário mudou.
const form = { original: {}, edited: {}, managed: new Set(), limits: {}, mount: '', restartDue: false };
// As pastas do computador (`LocalFolders`) e as contas, para o configurador.
const computer = { folders: [], accounts: [] };
// Os pedaços que mudam sem redesenhar e a conferência de cada campo com linha de erro.
const ui = { errors: {} };

export async function render(root, ctx) {
  const css = stylesheet('screens/preferences.css');
  Object.assign(screen, { root, ctx });
  // Mudanças de fora (a bandeja, a CLI) entram quando não há edição pendente; o eco do próprio
  // Salvar não muda nada e não redesenha.
  screen.listening ??= ctx.api.on('preferences-changed', () => {
    if (pending().length) return;
    loadPrefs()
      .then((changed) => changed && draw())
      .catch(() => {});
  });
  screen.listeningArg ??= listenArg(ctx);
  screen.listeningFolders ??= ctx.api.on('local-folders-changed', () => loadFolders().then(drawFolders).catch(() => {}));
  await Promise.all([loadPrefs(), loadContext(), loadFolders(), css]);
  draw();
  if (screen.folderArg) {
    const path = screen.folderArg;
    screen.folderArg = null;
    addFolder(path);
  }
}

// A aba pedida por quem abriu a janela: `?arg=` ao abrir, `screen-arg` com ela já aberta.
// `folder:<caminho>` abre "Meu computador" com o configurador dessa pasta.
function listenArg({ api }) {
  const arg = screenArg(location.search);
  screen.folderArg = folderArg(arg);
  if (screen.folderArg) screen.tab = 'computer';
  else if (TABS[arg]) screen.tab = arg;
  return api.on('screen-arg', (wanted) => {
    if (!ui.tabs) return;
    const folder = folderArg(wanted);
    if (folder) {
      showTab('computer');
      addFolder(folder);
    } else if (TABS[wanted]) showTab(wanted);
  });
}

// As preferências do serviço; `true` se algo mudou.
async function loadPrefs() {
  const { values, managed } = fromPrefs(await screen.ctx.api.call('preferences'));
  const changed = diff(form.original, values).length > 0 || [...managed].join() !== [...form.managed].join();
  Object.assign(form, { original: values, edited: { ...values }, managed });
  return changed;
}

// O que não depende das preferências: a pasta do Drive e o maior cache que cabe no disco (o que
// ele já ocupa mais o espaço livre).
async function loadContext() {
  const { api } = screen.ctx;
  const [status, summary] = await Promise.all([api.call('status'), api.call('offline_summary').catch(() => null)]);
  form.mount = status.mount_point;
  computer.accounts = status.accounts.map((a) => a.email);
  form.limits = { maxCacheGb: summary ? Math.floor((summary.cache_bytes + summary.free_bytes) / 1024 ** 3) : null };
}

async function loadFolders() {
  computer.folders = await screen.ctx.api.call('local_folders');
}

const pending = () => diff(form.original, form.edited);

const TABS = { computer: computerTab, drive: driveTab, settings: settingsTab };

function draw() {
  const { api, t } = screen.ctx;
  ui.tabs = Object.keys(TABS).map((tab) =>
    el('button', { role: 'tab', 'data-tab': tab, onclick: () => showTab(tab) }, t(`pref-tab-${tab}`)));
  ui.content = el('main', { class: 'tab-content', role: 'tabpanel' });
  ui.footer = saveFooter(t, { onSave: save, onCancel: cancel });
  ui.banner = el('div', { class: 'banner restart-banner', role: 'status', hidden: !form.restartDue },
    el('span', {}, t('pref-restart-needed')),
    el('button', { onclick: () => attempt(api.call('restart')) }, t('pref-restart-now')));
  screen.root.replaceChildren(
    el('div', { class: 'preferences' },
      el('nav', { class: 'tabs', role: 'tablist' }, ui.tabs),
      ui.content,
      ui.banner,
      ui.footer.footer),
  );
  showTab(screen.tab);
}

// Troca só o conteúdo da aba (o resto da página e o foco ficam).
function showTab(tab) {
  screen.tab = tab;
  for (const button of ui.tabs) button.setAttribute('aria-selected', String(button.dataset.tab === tab));
  ui.errors = {};
  ui.content.replaceChildren(...TABS[tab]());
  updateFooter();
}

function updateFooter() {
  const changes = pending();
  // Só o que mudou se valida: um valor antigo fora dos limites não impede salvar o resto.
  const invalid = changes.some(([key]) => validate(key, form.edited[key], form.limits));
  ui.footer.update(changes.length > 0, !invalid);
}

// Grava `patch` (`{ chave: valor }`) no que o usuário mudou e confere os campos.
function set(patch) {
  Object.assign(form.edited, patch);
  for (const key of Object.keys(patch)) ui.errors[key]?.();
  updateFooter();
}

function save() {
  const { api, t } = screen.ctx;
  attempt(api.call('set_preferences', { values: pending() }), (restart) => {
    form.original = { ...form.edited };
    form.restartDue ||= restart;
    ui.banner.hidden = !form.restartDue;
    toast(t('pref-saved'));
    updateFooter();
  });
}

function cancel() {
  form.edited = { ...form.original };
  showTab(screen.tab);
}

// Abas.

function computerTab() {
  const { t } = screen.ctx;
  // A política desliga a sincronização de pastas: o botão fica bloqueado com o aviso de sempre.
  const disabled = form.managed.has('DisableMirroredFolders') && form.original.DisableMirroredFolders === true;
  ui.folders = el('ul', { class: 'local-folders' });
  drawFolders();
  return [
    el('p', { class: 'meta' }, t('pref-computer-text')),
    ui.folders,
    el('fieldset', { class: 'pref', 'data-key': 'DisableMirroredFolders', disabled },
      el('button', { class: 'tonal add-folder', onclick: () => pickAndAdd() }, t('pref-add-folder')),
      disabled ? el('span', { class: 'meta managed-note' }, t('folder-refused-folders-disabled')) : []),
  ];
}

// A lista das pastas do computador (redesenhada quando o serviço avisa que mudou).
function drawFolders() {
  ui.folders?.replaceChildren(...folderItems(screen.ctx, computer.folders, computer.accounts.length > 1));
}

async function pickAndAdd() {
  const dir = await screen.ctx.api.pickFolder(screen.ctx.t('pref-pick-folder')).catch(() => null);
  if (dir) addFolder(dir);
}

// O configurador da pasta `path`: a conta, o tamanho e a contagem (ou o motivo da recusa), a
// sincronização com o Drive e, se a pasta já está em "Computadores", se junta com ela.
async function addFolder(path) {
  const { api, t, locale } = screen.ctx;
  const name = folderName(path);
  const account = el('select', { 'aria-label': t('folder-config-account') },
    computer.accounts.map((email) => el('option', { value: email, selected: email === currentAccount() }, email)));
  const summary = el('p', { class: 'folder-summary' });
  const sync = el('input', { type: 'checkbox', class: 'folder-sync', checked: true });
  const merge = el('input', { type: 'checkbox', class: 'folder-merge', checked: true });
  const mergeRow = choice(merge, t('folder-config-merge'));
  let state = null;
  const check = async () => {
    state = null;
    summary.textContent = '';
    mergeRow.hidden = true;
    const reply = await api.call('check_local_folder', { account: account.value, path }).catch(toastError);
    if (!reply) return;
    state = configurator(reply);
    summary.textContent = state.refusal
      ? t(state.refusal)
      : t('folder-config-summary', { files: state.files, size: formatSize(state.size, locale) });
    summary.classList.toggle('field-error', Boolean(state.refusal));
    mergeRow.hidden = !state.askMerge;
    // O diálogo reavalia o Concluído a cada `input`.
    body.dispatchEvent(new Event('input', { bubbles: true }));
  };
  account.addEventListener('change', check);
  const body = el('div', { class: 'folder-config' },
    el('p', { class: 'path' }, path),
    computer.accounts.length > 1 ? el('label', { class: 'field' }, el('span', {}, t('folder-config-account')), account) : [],
    summary,
    choice(sync, t('folder-config-sync')),
    mergeRow);
  const answer = confirmDialog({
    title: t('folder-config-title'),
    body,
    confirm: t('folder-config-done'),
    cancel: t('action-cancel'),
    ready: () => Boolean(state && !state.refusal && sync.checked),
  });
  await check();
  if (!(await answer)) return;
  const args = { account: account.value, path, merge: state.askMerge && merge.checked };
  api.call('add_local_folder', args)
    .then(() => toast(t('folder-added', { name })))
    .catch(refusalToast(t));
}

function driveTab() {
  const { api, t } = screen.ctx;
  const mode = (value, label, text, disabled) =>
    choice(el('input', { type: 'radio', name: 'drive-mode', value, checked: value === 'stream', disabled }),
      el('strong', {}, label), el('br'), text,
      disabled ? [el('br'), el('span', { class: 'meta' }, t('pref-coming-soon'))] : []);
  return [
    el('h2', {}, t('pref-drive-mode')),
    el('div', { class: 'choices' },
      mode('stream', t('pref-stream'), t('pref-stream-text'), false),
      mode('mirror', t('pref-mirror'), t('pref-mirror-text'), true)),
    el('h2', {}, t('pref-drive-folder')),
    el('p', { class: 'path' }, form.mount),
    el('button', { class: 'tonal', onclick: () => attempt(api.openMount()) }, t('pref-open-folder')),
    el('button', { class: 'tonal manage-shared-drives', onclick: () => api.openScreen('shared-drives', currentAccount()) },
      t('pref-shared-drives')),
  ];
}

function settingsTab() {
  const { api, t } = screen.ctx;
  const section = (title, ...rows) => el('section', { class: 'pref-section' }, el('h2', {}, t(title)), ...rows);
  return [
    section('pref-section-location',
      folder('DefaultMountPoint', 'pref-mount', form.mount),
      checkbox('ShowMountPointInSidebar', 'pref-sidebar')),
    section('pref-section-cache',
      folder('ContentCachePath', 'pref-cache-path', t('pref-default')),
      cacheLimit(),
      el('button', { class: 'text', onclick: () => api.openScreen('offline') }, t('pref-cache-storage'))),
    section('pref-section-system',
      checkbox('AutoStartOnLogin', 'pref-autostart'),
      checkbox('PauseSyncingOnMeteredConnection', 'pref-metered'),
      theme()),
    section('pref-section-network', proxy(), limit('BandwidthRxKBPS', 'pref-bandwidth-rx'), limit('BandwidthTxKBPS', 'pref-bandwidth-tx')),
    section('pref-section-other',
      hotkey(),
      checkbox('AutomaticErrorReporting', 'pref-error-reports', 'pref-error-reports-hint')),
    section('pref-account',
      el('button', { class: 'tonal danger', onclick: () => disconnect(screen.ctx) }, t('account-disconnect'))),
  ];
}

// Controles. Cada um vem numa linha `.pref[data-key]`; com uma das chaves na política, a linha é
// um `fieldset` desabilitado (o navegador bloqueia todos os campos) e diz "Gerenciado pelo
// administrador".

function row(keys, ...children) {
  const { t } = screen.ctx;
  const list = [keys].flat();
  const managed = list.some((k) => form.managed.has(k));
  return el('fieldset', { class: 'pref', 'data-key': list[0], disabled: managed }, ...children,
    managed ? el('span', { class: 'meta managed-note' }, t('pref-managed')) : []);
}

// A linha de erro de `key`, embaixo do campo; `set` a confere a cada mudança.
function errorLine(key) {
  const line = el('p', { class: 'field-error', role: 'alert' });
  ui.errors[key] = () => {
    const error = validate(key, form.edited[key], form.limits);
    line.textContent = error ? screen.ctx.t(...error) : '';
  };
  ui.errors[key]();
  return line;
}

function checkbox(key, label, hint) {
  const { t } = screen.ctx;
  const input = el('input', { type: 'checkbox', checked: form.edited[key] === true });
  input.addEventListener('change', () => set({ [key]: input.checked }));
  return row(key, choice(input, t(label), hint ? [el('br'), el('span', { class: 'meta' }, t(hint))] : []));
}

// Uma pasta com Alterar (o seletor do sistema) e Redefinir (o padrão, `fallback` na tela).
function folder(key, label, fallback) {
  const { api, t } = screen.ctx;
  const path = el('span', { class: 'path' }, form.edited[key] ?? fallback);
  const choose = (dir) => {
    path.textContent = dir ?? fallback;
    set({ [key]: dir });
  };
  const change = async () => {
    const dir = await api.pickFolder(t('pref-pick-folder')).catch(() => null);
    if (dir) choose(dir);
  };
  return row(key, el('div', { class: 'field' }, el('span', {}, t(label)), path),
    el('div', { class: 'row-buttons' },
      el('button', { class: 'text', onclick: change }, t('pref-change')),
      el('button', { class: 'text', onclick: () => choose(null) }, t('pref-reset'))));
}

function cacheLimit() {
  const { t } = screen.ctx;
  const key = 'ContentCacheMaxKbytes';
  const max = form.limits.maxCacheGb;
  const input = el('input', { type: 'number', min: 1, max: max ?? false, step: 1, value: kbToGb(form.edited[key]) ?? '' });
  input.addEventListener('input', () => set({ [key]: gbToKb(numberOrNull(input.value)) }));
  return row(key, el('label', { class: 'field' }, el('span', {}, t('pref-cache-limit')), input),
    max === null ? [] : el('p', { class: 'meta' }, t('pref-cache-limit-hint', { max })),
    errorLine(key));
}

// Banda: a caixa liga o limite; vazia ou desligada, sem limite (`null`).
function limit(key, label) {
  const { t } = screen.ctx;
  const value = form.edited[key];
  const on = el('input', { type: 'checkbox', checked: value !== null });
  const input = el('input', { type: 'number', min: 1, step: 1, value: value ?? '', disabled: value === null, 'aria-label': t(label) });
  const update = () => {
    input.disabled = !on.checked;
    set({ [key]: on.checked ? numberOrNull(input.value) : null });
  };
  on.addEventListener('change', update);
  input.addEventListener('input', update);
  return row(key, choice(on, t(label)), input, errorLine(key));
}

// Proxy: automático, direto ou manual, nas duas chaves (`proxyPatch`).
function proxy() {
  const { t } = screen.ctx;
  const current = proxyMode(form.edited);
  const address = el('input', {
    type: 'text', value: form.edited.ProxyServer ?? '', placeholder: 'http://servidor:3128',
    'aria-label': t('pref-proxy-server'), hidden: current !== 'manual',
  });
  const options = ['auto', 'direct', 'manual'].map((value) =>
    el('input', { type: 'radio', name: 'proxy', value, checked: value === current }));
  const update = () => {
    const mode = options.find((o) => o.checked).value;
    address.hidden = mode !== 'manual';
    set(proxyPatch(mode, address.value));
  };
  for (const option of options) option.addEventListener('change', update);
  address.addEventListener('input', update);
  return row(['ProxyServer', 'DirectConnection'], el('span', {}, t('pref-proxy')),
    el('div', { class: 'choices' }, options.map((o) => choice(o, t(`pref-proxy-${o.value}`)))),
    address, errorLine('ProxyServer'));
}

// O atalho: o campo captura a combinação (Tab e Esc passam; Backspace volta ao padrão).
function hotkey() {
  const { t } = screen.ctx;
  const key = 'SearchHotKey';
  const input = el('input', { type: 'text', readonly: true, value: form.edited[key] ?? DEFAULT_HOTKEY, 'aria-label': t('pref-hotkey') });
  const choose = (combo) => {
    input.value = combo ?? DEFAULT_HOTKEY;
    set({ [key]: combo });
  };
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Tab' || e.key === 'Escape') return;
    e.preventDefault();
    const combo = hotkeyFrom(e);
    if (e.key === 'Backspace' && !e.ctrlKey && !e.altKey) choose(null);
    else if (combo) choose(combo);
  });
  return row(key, el('label', { class: 'field' }, el('span', {}, t('pref-hotkey')), input), el('p', { class: 'meta' }, t('pref-hotkey-hint')));
}

function theme() {
  const { t } = screen.ctx;
  const select = el('select', { 'aria-label': t('pref-theme') },
    el('option', { value: '', selected: form.edited.UITheme !== 'light' }, t('pref-theme-system')),
    el('option', { value: 'light', selected: form.edited.UITheme === 'light' }, t('pref-theme-light')));
  select.addEventListener('change', () => set({ UITheme: select.value || null }));
  return row('UITheme', el('label', { class: 'field' }, el('span', {}, t('pref-theme')), select));
}
