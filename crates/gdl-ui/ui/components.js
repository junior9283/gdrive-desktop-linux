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

// Menu suspenso abaixo de `anchor`: `items` são `{ label, action, disabled, id }`, `{ node }`
// (conteúdo sem ação, como o cartão da conta) ou `'-'` (separador). Fecha ao escolher, ao clicar
// fora ou com Esc.
export function popupMenu(anchor, items) {
  document.querySelector('.menu')?.remove();
  const menu = el('div', { class: 'menu', role: 'menu' });
  const close = () => {
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
  const box = anchor.getBoundingClientRect();
  const left = Math.min(box.left, window.innerWidth - menu.offsetWidth - 8);
  menu.style.left = `${Math.max(8, left)}px`;
  menu.style.top = `${box.bottom + 4}px`;
  document.addEventListener('pointerdown', outside, true);
  document.addEventListener('keydown', escape, true);
  menu.querySelector('button:not([disabled])')?.focus();
}

// Aviso curto no rodapé da janela.
export function toast(text) {
  const note = el('div', { class: 'toast', role: 'status' }, text);
  document.body.append(note);
  setTimeout(() => note.remove(), 4000);
}

// O erro de uma ação do usuário, num aviso.
export const toastError = (error) => toast(error?.message ?? String(error));

// Espera `promise`; com sucesso chama `done` (com o resultado), com erro mostra o aviso.
export const attempt = (promise, done) => promise.then(done).catch(toastError);
