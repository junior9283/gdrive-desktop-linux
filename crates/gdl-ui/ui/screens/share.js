// Compartilhar (aberta pelo menu do gerenciador de arquivos com o id do item): adicionar
// pessoas com um papel e, se quiser, um aviso por e-mail com mensagem; mudar o papel de quem já
// tem acesso (ou tirar o acesso); o acesso geral (restrito, domínio ou qualquer pessoa com o
// link); e copiar o link. A lógica sem DOM fica em `share/model.js`.

import { attempt, choice, el, icon, stylesheet, toast } from '../components.js';
import { itemArg } from '../router.js';
import { ROLES, generalKinds, parseEmails } from './share/model.js';

const GENERAL_ICONS = { restricted: 'lock', domain: 'domain', anyone: 'public' };

// O estado entre as chamadas de `render` (ela é chamada de novo quando o serviço volta). `people`
// e `general` são as partes redesenhadas a cada mudança (o que foi digitado em "Adicionar
// pessoas" fica).
const screen = { ctx: null, root: null, account: '', id: null, email: '', people: null, general: null };

export async function render(root, ctx) {
  Object.assign(screen, { ctx, root, ...itemArg(location.search) });
  const css = stylesheet('screens/share.css');
  if (!screen.id) {
    await css;
    root.replaceChildren(el('main', { class: 'share' }, el('p', { class: 'empty' }, ctx.t('share-missing'))));
    return;
  }
  const [data] = await Promise.all([load(), css]);
  draw(data);
}

// O item da tela, nos argumentos de toda chamada.
const item = () => ({ account: screen.account, item_id: screen.id });

const load = () => screen.ctx.api.call('sharing', item());

// Faz a mudança e redesenha quem tem acesso e o acesso geral com o que o Drive devolveu.
const change = (promise, done) =>
  attempt(promise.then(load), (data) => {
    if (done) toast(done);
    refresh(data);
  });

function refresh(data) {
  screen.people.replaceChildren(...data.people.map((person) => personRow(person, data.can_share)));
  screen.general.replaceWith((screen.general = generalAccess(data)));
}

function draw(data) {
  const { api, t } = screen.ctx;
  screen.people = el('ul', { class: 'list people' });
  screen.general = el('section');
  refresh(data);
  screen.root.replaceChildren(
    el(
      'main',
      { class: 'share' },
      el('h1', { class: 'name' }, t('share-title', { name: data.name })),
      data.can_share ? addPeople() : el('p', { class: 'meta' }, t('share-cannot')),
      el('h2', {}, t('share-people')),
      screen.people,
      el('h2', {}, t('share-general')),
      screen.general,
      el(
        'div',
        { class: 'actions' },
        el('button', { class: 'text copy-link', onclick: () => attempt(api.copy(data.link), () => toast(t('link-copied'))) },
          icon('link'), t('share-copy-link')),
        el('button', { class: 'filled', onclick: () => api.closeWindow() }, t('share-done')),
      ),
    ),
  );
}

function roleSelect(label, value, extra = []) {
  const { t } = screen.ctx;
  return el('select', { 'aria-label': label },
    [...ROLES, ...extra].map((role) => el('option', { value: role, selected: role === value }, t(`share-role-${role}`))));
}

// O campo de e-mails, o papel, o aviso por e-mail (com a mensagem) e o botão de enviar.
function addPeople() {
  const { api, t } = screen.ctx;
  // O e-mail que veio pedido (servidor MCP) fica digitado; quem envia é a pessoa.
  const emails = el('input', { type: 'text', class: 'emails', value: screen.email, placeholder: t('share-add-placeholder'), 'aria-label': t('share-add') });
  const role = roleSelect(t('share-role'), 'reader');
  const notify = el('input', { type: 'checkbox', checked: true });
  const message = el('textarea', { class: 'message', rows: 3, placeholder: t('share-message') });
  notify.addEventListener('change', () => {
    message.hidden = !notify.checked;
  });
  const send = () => {
    const { emails: list, invalid } = parseEmails(emails.value);
    if (invalid.length) return toast(t('share-invalid', { emails: invalid.join(', ') }));
    if (!list.length) return emails.focus();
    const added = api.call('share_add', { ...item(), emails: list, role: role.value, notify: notify.checked, message: message.value });
    // Com sucesso, o campo e a mensagem ficam livres para as próximas pessoas.
    change(added.then(() => {
      emails.value = '';
      message.value = '';
    }), t('share-added', { count: list.length }));
  };
  emails.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') send();
  });
  return el(
    'section',
    { class: 'add' },
    el('div', { class: 'row' }, emails, role),
    choice(notify, t('share-notify')),
    message,
    el('button', { class: 'filled send', onclick: send }, t('share-send')),
  );
}

// Uma pessoa com acesso: o dono sem escolha; os outros com o papel e "Remover acesso".
function personRow(person, canShare) {
  const { api, t } = screen.ctx;
  const who = el('div', {},
    el('span', { class: 'name' }, person.name || person.email),
    person.name ? el('span', { class: 'meta name' }, person.email) : []);
  const fixed = person.role === 'owner' || !canShare;
  const control = fixed
    ? el('span', { class: 'meta' }, t(`share-role-${person.role}`))
    : roleSelect(t('share-role-of', { name: person.name || person.email }), person.role, ['remove']);
  control.addEventListener('change', () => {
    const role = control.value === 'remove' ? '' : control.value;
    change(api.call('share_set_role', { ...item(), permission_id: person.id, role }), role ? null : t('share-removed'));
  });
  return el('li', { 'data-email': person.email }, who, control);
}

// O acesso geral: o tipo, o papel (fora no restrito) e o que ele quer dizer.
function generalAccess(data) {
  const { api, t } = screen.ctx;
  const kinds = generalKinds(data.domain);
  const kind = el('select', { class: 'general-kind', 'aria-label': t('share-general'), disabled: !data.can_share },
    kinds.map((k) => el('option', { value: k, selected: k === data.general }, t(`share-general-${k}`, { domain: data.domain }))));
  const role = roleSelect(t('share-general-role'), data.general_role || 'reader');
  role.disabled = !data.can_share;
  role.hidden = data.general === 'restricted';
  const apply = () =>
    change(api.call('share_set_general', { ...item(), kind: kind.value, role: kind.value === 'restricted' ? '' : role.value }));
  kind.addEventListener('change', apply);
  role.addEventListener('change', apply);
  return el(
    'section',
    { class: 'general' },
    icon(GENERAL_ICONS[data.general] ?? 'lock'),
    el('div', {},
      el('div', { class: 'row' }, kind, role),
      el('p', { class: 'meta' }, t(`share-general-${data.general}-text`, { domain: data.domain }))),
  );
}
