// O tour da janela principal, sem DOM: cada passo aponta para um elemento (`target`) e tem título
// e texto (chaves do `ui.ftl`).

export const HOME_TOUR = [
  { target: '.header', title: 'tour-status-title', text: 'tour-status-text' },
  { target: '.search', title: 'tour-search-title', text: 'tour-search-text' },
  { target: '.nav', title: 'tour-nav-title', text: 'tour-nav-text' },
  { target: '.account-button', title: 'tour-account-title', text: 'tour-account-text' },
  { target: '.more-button', title: 'tour-more-title', text: 'tour-more-text' },
];

// Se o elemento aparece na tela: um que existe mas está escondido (`hidden`) não tem caixa.
export function onScreen(node) {
  return !!node && node.getClientRects().length > 0;
}

// Os passos cujo alvo está na tela (`exists(seletor)`): os outros são pulados.
export function visibleSteps(steps, exists) {
  return steps.filter((step) => exists(step.target));
}
