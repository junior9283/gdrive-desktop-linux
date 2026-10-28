// Enviar feedback (Mais opções e bandeja): o tipo, a descrição e, se quiser, o pacote de
// diagnóstico. "Enviar" abre no navegador uma issue do projeto já preenchida; o pacote não vai
// pelo endereço (o GitHub não aceita anexo assim): o serviço o grava, mostra a pasta e o texto
// pede para anexá-lo.

import { attempt, choice, el, stylesheet, toast } from '../components.js';
import { canSend, feedbackIssue } from './feedback/model.js';

const KINDS = ['problem', 'idea'];

export async function render(root, { api, t }) {
  const css = stylesheet('screens/feedback.css');
  const kinds = KINDS.map((value) =>
    el('input', { type: 'radio', name: 'feedback-kind', value, checked: value === 'problem' }));
  const text = el('textarea', { class: 'feedback-text', rows: 8, 'aria-label': t('feedback-text'), placeholder: t('feedback-placeholder') });
  const diagnostics = el('input', { type: 'checkbox', class: 'feedback-diagnostics' });
  const send = el('button', { class: 'filled send', disabled: true }, t('feedback-send'));
  text.addEventListener('input', () => { send.disabled = !canSend(text.value); });

  async function submit() {
    send.disabled = true;
    try {
      const file = diagnostics.checked ? await api.call('generate_diagnostics') : '';
      const kind = kinds.find((k) => k.checked).value;
      const { title, body } = feedbackIssue({ kind, text: text.value, diagnostics: file }, t);
      return await api.openIssue(title, body);
    } finally {
      send.disabled = false;
    }
  }
  // Com o texto cortado, a janela fica aberta com o aviso, para o usuário completar no navegador.
  send.addEventListener('click', () =>
    attempt(submit(), (cut) => (cut ? toast(t('feedback-cut')) : api.closeWindow())));

  await css;
  root.replaceChildren(
    el('div', { class: 'pick-screen feedback' },
      el('main', {},
        el('h1', {}, t('feedback-title')),
        el('p', { class: 'meta' }, t('feedback-intro')),
        el('fieldset', { class: 'feedback-kinds' },
          el('legend', {}, t('feedback-kind')),
          kinds.map((input) => choice(input, t(`feedback-kind-${input.value}`)))),
        text,
        choice(diagnostics, t('feedback-include-diagnostics')),
        el('p', { class: 'meta' }, t('feedback-public'))),
      el('footer', { class: 'actions save-footer' },
        el('button', { class: 'tonal cancel', onclick: () => api.closeWindow() }, t('action-cancel')),
        send)),
  );
  text.focus();
}
