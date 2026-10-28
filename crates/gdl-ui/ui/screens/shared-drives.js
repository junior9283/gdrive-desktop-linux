// Drives compartilhados (aberta pelas Preferências e pelo menu da pasta de mesmo nome, com a
// conta): uma chave para ligar ou desligar cada Drive, "Novo Drive compartilhado" e o rodapé com
// "Alterações pendentes", Cancelar e Salvar (que manda a lista toda). Com envio pendente num drive
// que seria desligado o serviço recusa, e o motivo aparece na tela. A lógica sem DOM fica em
// `shared-drives/model.js`.

import { attempt, choice, confirmDialog, el, saveFooter, stylesheet, toast } from '../components.js';
import { screenArg } from '../router.js';
import { currentAccount } from '../state.js';
import { applied, changed, isOn, parseArg, saveDrives, shownAccount } from './shared-drives/model.js';

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta). `drives`
// é a lista do serviço e `overrides` o que as chaves mudaram dela (id → ligado). `accounts` só é
// lida quando a tela abriu sem conta (da bandeja); com mais de uma, a tela mostra a escolha.
const screen = { ctx: null, root: null, account: '', accounts: [], drives: [], overrides: {}, ui: {} };

export async function render(root, ctx) {
  const { account: arg, create } = parseArg(screenArg(location.search));
  const css = stylesheet('screens/shared-drives.css');
  const status = arg ? null : await ctx.api.call('status', { account: '' });
  const accounts = status?.accounts ?? [];
  Object.assign(screen, { ctx, root, accounts });
  await Promise.all([load(shownAccount(arg, currentAccount(), accounts)), css]);
  draw();
  if (create) newDrive();
}

async function load(account) {
  screen.account = account;
  screen.drives = await screen.ctx.api.call('shared_drives', { account });
}

function draw() {
  const { t } = screen.ctx;
  const ui = screen.ui;
  ui.error = el('p', { class: 'field-error', role: 'alert' });
  ui.list = el('ul', { class: 'list pick-list drive-list' });
  ui.footer = saveFooter(t, { onSave: save, onCancel: reset });
  screen.root.replaceChildren(
    el('div', { class: 'pick-screen shared-drives' },
      el('main', {},
        el('h1', {}, t('shared-drives-title')),
        el('p', { class: 'meta' }, t('shared-drives-text')),
        accountPicker(),
        el('button', { class: 'tonal new-drive', onclick: newDrive }, t('shared-drives-new')),
        ui.list,
        ui.error),
      ui.footer.footer),
  );
  reset();
}

function accountPicker() {
  if (screen.accounts.length < 2) return '';
  const { t } = screen.ctx;
  const select = el('select', { class: 'drive-account', 'aria-label': t('shared-drives-account') },
    screen.accounts.map(({ email }) => el('option', { value: email, selected: email === screen.account }, email)));
  // Outra conta: a lista dela, sem as chaves mexidas na anterior.
  select.addEventListener('change', () => attempt(load(select.value), reset));
  return el('label', { class: 'field' }, el('span', {}, t('shared-drives-account')), select);
}

// Volta as chaves ao estado do serviço (Cancelar, e depois de Salvar, que já o atualizou).
function reset() {
  screen.overrides = {};
  screen.ui.error.textContent = '';
  drawList();
}

// A lista, quando `drives` ou as chaves mudam por fora de um clique.
function drawList() {
  const { t } = screen.ctx;
  screen.ui.list.replaceChildren(
    ...(screen.drives.length ? screen.drives.map(row) : [el('li', { class: 'empty' }, t('shared-drives-empty'))]),
  );
  updateFooter();
}

const updateFooter = () => screen.ui.footer.update(changed(screen.drives, screen.overrides));

// Um clique só mexe no rodapé: redesenhar a lista perderia o foco do teclado.
function row(drive) {
  const input = el('input', { type: 'checkbox', role: 'switch', checked: isOn(drive, screen.overrides) });
  input.addEventListener('change', () => {
    screen.overrides[drive.id] = input.checked;
    screen.ui.error.textContent = '';
    updateFooter();
  });
  return el('li', { 'data-drive': drive.id }, choice(input, el('span', { class: 'name' }, drive.name)));
}

function save() {
  const { api, t } = screen.ctx;
  const { account, drives, overrides, ui } = screen;
  attempt(saveDrives(api, account, drives, overrides), (result) => {
    if (!result.ok) {
      ui.error.textContent = result.message;
      return;
    }
    screen.drives = applied(drives, overrides);
    reset();
    toast(t('shared-drives-saved'));
  });
}

// Pede o nome e cria o drive, que entra na lista como o serviço o devolveu (com o sufixo, se o nome
// já existia) e fica como o usuário o deixar: as chaves já mexidas continuam.
async function newDrive() {
  const { api, t } = screen.ctx;
  const name = el('input', { type: 'text', class: 'drive-name', 'aria-label': t('shared-drives-name'), placeholder: t('shared-drives-name') });
  const ok = await confirmDialog({
    title: t('shared-drives-new'),
    body: name,
    confirm: t('shared-drives-create'),
    cancel: t('action-cancel'),
    ready: () => name.value.trim() !== '',
  });
  if (!ok) return;
  attempt(api.call('create_shared_drive', { account: screen.account, name: name.value }), (made) => {
    screen.drives = [...screen.drives, made];
    drawList();
    toast(t('shared-drives-created', { name: made.name }));
  });
}
