// Peças de tela, montadas pelo DOM (sem `innerHTML` e sem estilo embutido, por causa da CSP).

const SVG = 'http://www.w3.org/2000/svg';

// `el('p', { class: 'x', onclick }, 'texto', outroElemento)`.
export function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
    else if (value !== false && value != null) node.setAttribute(key, value === true ? '' : value);
  }
  node.append(...children.flat());
  return node;
}

// Um símbolo do `icons/sprite.svg`.
export function icon(name) {
  const svg = document.createElementNS(SVG, 'svg');
  svg.setAttribute('class', 'icon');
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG, 'use');
  use.setAttribute('href', `icons/sprite.svg#${name}`);
  svg.append(use);
  return svg;
}

// Uma caixa de seleção ou opção com o texto ao lado (`.choice`).
export function choice(input, ...content) {
  return el('label', { class: 'choice' }, input, el('span', {}, ...content));
}

// Um cartão: título, conteúdo e, se houver, um botão de texto.
export function card(title, content, label, onclick) {
  return el(
    'section',
    { class: 'card' },
    el('h2', {}, title),
    content,
    label ? el('button', { class: 'text', onclick }, label) : [],
  );
}

// Carrega a folha de estilo de uma tela (uma vez); resolve quando ela vale, para a tela não
// aparecer sem estilo.
export function stylesheet(href) {
  if (document.querySelector(`link[href="${href}"]`)) return Promise.resolve();
  return new Promise((resolve) => {
    document.head.append(el('link', { rel: 'stylesheet', href, onload: resolve, onerror: resolve }));
  });
}

// Caixa de diálogo modal: resolve `true` com o botão de confirmar e `false` ao cancelar (ou Esc).
// `ready()` diz se dá para confirmar; vale de novo a cada mudança no conteúdo.
export function confirmDialog({ title, body, confirm, cancel, danger = false, ready = () => true }) {
  return new Promise((resolve) => {
    const ok = el('button', { class: danger ? 'filled danger' : 'filled' }, confirm);
    const no = el('button', { class: 'dialog-cancel' }, cancel);
    const dialog = el(
      'dialog',
      { class: 'dialog' },
      el('h2', { class: 'dialog-title' }, title),
      el('div', { class: 'dialog-body' }, body),
      el('div', { class: 'actions' }, no, ok),
    );
    const finish = (answer) => {
      dialog.close();
      dialog.remove();
      resolve(answer);
    };
    ok.addEventListener('click', () => finish(true));
    no.addEventListener('click', () => finish(false));
    dialog.addEventListener('cancel', (e) => {
      e.preventDefault();
      finish(false);
    });
    const check = () => {
      ok.disabled = !ready();
    };
    dialog.addEventListener('change', check);
    dialog.addEventListener('input', check);
    check();
    document.body.append(dialog);
    dialog.showModal();
  });
}

// Menu suspenso abaixo de `anchor`: `items` são `{ label, action, disabled, id }` (`label` pode ser
// um nó, como a linha de uma conta), `{ node }`
// (conteúdo sem ação, como o cartão da conta) ou `'-'` (separador). Fecha ao escolher, ao clicar
// fora ou com Esc.
let closeOpenMenu = null;
export function popupMenu(anchor, items) {
  closeOpenMenu?.();
  const menu = el('div', { class: 'menu', role: 'menu' });
  // Recalcula a posição quando o conteúdo muda de tamanho (o menu da conta alarga quando chegam
  // a foto e o nome): senão ele sai pela borda da janela.
  const place = () => {
    const box = anchor.getBoundingClientRect();
    const left = Math.min(box.left, window.innerWidth - menu.offsetWidth - 8);
    menu.style.left = `${Math.max(8, left)}px`;
    menu.style.top = `${box.bottom + 4}px`;
  };
  const resized = new ResizeObserver(place);
  const close = () => {
    if (closeOpenMenu === close) closeOpenMenu = null;
    resized.disconnect();
    menu.remove();
    document.removeEventListener('pointerdown', outside, true);
    document.removeEventListener('keydown', escape, true);
  };
  const outside = (e) => {
    if (!menu.contains(e.target)) close();
  };
  const escape = (e) => {
    if (e.key === 'Escape') close();
  };
  for (const item of items) {
    if (item === '-' || item.node) {
      menu.append(item.node ?? el('hr'));
      continue;
    }
    const button = el(
      'button',
      { role: 'menuitem', 'data-action': item.id, disabled: item.disabled },
      item.label,
    );
    button.addEventListener('click', () => {
      close();
      item.action();
    });
    menu.append(button);
  }
  document.body.append(menu);
  closeOpenMenu = close;
  place();
  resized.observe(menu);
  document.addEventListener('pointerdown', outside, true);
  document.addEventListener('keydown', escape, true);
  menu.querySelector('button:not([disabled])')?.focus();
}

// O botão "⋮" de uma linha: abre `popupMenu` com os itens de `items()`, montados no clique.
export function menuButton(label, items) {
  const button = el('button', { class: 'icon-button row-menu', 'aria-label': label }, icon('more_vert'));
  button.addEventListener('click', () => popupMenu(button, items()));
  return button;
}

// A paginação de `paginate`: o que vier em `extra` (o tamanho da página), "Mostrando X–Y de Z" e
// as setas, que chamam `go(-1)` ou `go(1)`.
export function pager(t, page, go, ...extra) {
  return el(
    'div',
    { class: 'pager' },
    ...extra,
    el('span', {}, t('showing', page)),
    el('button', { class: 'icon-button', 'aria-label': t('page-previous'), disabled: page.page <= 1, onclick: () => go(-1) }, icon('chevron_left')),
    el('button', { class: 'icon-button', 'aria-label': t('page-next'), disabled: page.page >= page.pages, onclick: () => go(1) }, icon('chevron_right')),
  );
}

// O rodapé "Alterações pendentes", Cancelar e Salvar das telas que editam e salvam (Preferências e
// Drives compartilhados): `footer` é o elemento e `update(changed, valid)` liga os botões (Salvar
// também pede `valid`).
export function saveFooter(t, { onSave, onCancel }) {
  const pending = el('span', { class: 'pending-changes' }, t('pref-pending'));
  const cancel = el('button', { class: 'tonal cancel', onclick: onCancel }, t('action-cancel'));
  const save = el('button', { class: 'filled save', onclick: onSave }, t('pref-save'));
  return {
    footer: el('footer', { class: 'actions save-footer' }, pending, cancel, save),
    update(changed, valid = true) {
      pending.hidden = !changed;
      cancel.disabled = !changed;
      save.disabled = !changed || !valid;
    },
  };
}

// Aviso curto no rodapé da janela.
export function toast(text) {
  const note = el('div', { class: 'toast', role: 'status' }, text);
  document.body.append(note);
  setTimeout(() => note.remove(), 4000);
}

// O texto de um erro (do serviço ou do JavaScript).
export const errorText = (error) => error?.message ?? String(error);

// O erro de uma ação do usuário, num aviso.
export const toastError = (error) => toast(errorText(error));

// Espera `promise`; com sucesso chama `done` (com o resultado), com erro mostra o aviso.
export const attempt = (promise, done) => promise.then(done).catch(toastError);
