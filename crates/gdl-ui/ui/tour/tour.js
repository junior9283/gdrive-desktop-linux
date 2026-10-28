// O tour: um balão por passo, junto do elemento que ele explica (destacado), com Voltar,
// Próximo/Concluir e Fechar; Esc fecha. Os passos e o que pular ficam em `model.js`.

import { el, icon, stylesheet } from '../components.js';
import { visibleSteps } from './model.js';

// O tour aberto: um novo fecha o anterior.
let closeOpen = null;

export async function startTour(steps, t) {
  closeOpen?.();
  const shown = visibleSteps(steps, (selector) => document.querySelector(selector) !== null);
  if (!shown.length) return;
  await stylesheet('tour/tour.css');
  let at = 0;
  let target = null;
  const balloon = el('div', { class: 'tour', role: 'dialog', 'aria-modal': 'false' });
  const close = () => {
    if (closeOpen === close) closeOpen = null;
    target?.classList.remove('tour-target');
    balloon.remove();
    document.removeEventListener('keydown', escape, true);
  };
  const escape = (e) => {
    if (e.key === 'Escape') close();
  };
  const go = (delta) => {
    at += delta;
    if (at >= shown.length) close();
    else draw();
  };
  const draw = () => {
    const step = shown[at];
    target?.classList.remove('tour-target');
    target = document.querySelector(step.target);
    target.classList.add('tour-target');
    const last = at === shown.length - 1;
    balloon.setAttribute('aria-label', t(step.title));
    balloon.replaceChildren(
      el('button', { class: 'icon-button tour-close', 'aria-label': t('action-close'), onclick: close }, icon('close')),
      el('h2', {}, t(step.title)),
      el('p', {}, t(step.text)),
      el('div', { class: 'tour-actions' },
        el('span', { class: 'meta' }, `${at + 1}/${shown.length}`),
        at > 0 ? el('button', { class: 'text tour-back', onclick: () => go(-1) }, t('onboarding-back')) : [],
        el('button', { class: 'filled tour-next', onclick: () => go(1) }, t(last ? 'tour-done' : 'tour-next'))),
    );
    const box = target.getBoundingClientRect();
    balloon.style.top = `${Math.min(box.bottom + 8, window.innerHeight - balloon.offsetHeight - 8)}px`;
    balloon.style.left = `${Math.max(8, Math.min(box.left, window.innerWidth - balloon.offsetWidth - 8))}px`;
    balloon.querySelector('.tour-next').focus();
  };
  document.body.append(balloon);
  closeOpen = close;
  document.addEventListener('keydown', escape, true);
  draw();
}
