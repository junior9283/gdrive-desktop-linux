// A lógica do primeiro uso sem DOM: em que passo está, para onde cada botão (ou o fim do login)
// leva e quais botões aparecem. Os erros saem como `[chave, argumentos]` do `ui.ftl`.

// Os passos da 0.2 (escolher pastas e o resumo chegam na 0.4, com o backup de pastas).
const LOGIN = ['sign-in', 'waiting'];
const TOUR = ['intro', 'files', 'offline'];

// Os passos mostrados: com `DisableOnboardingDialog`, só os do login.
export function stepsFor(loginOnly) {
  return loginOnly ? [...LOGIN] : [...LOGIN, ...TOUR];
}

// Onde começa: sem conta, no login; com a conta conectada, na apresentação (ou no fim, se ela
// está desligada). `mount` é a pasta do Drive.
export function firstState({ signedIn, loginOnly, mount = '' }) {
  const step = !signedIn ? 'sign-in' : loginOnly ? 'end' : 'intro';
  return { step, loginOnly, mount, clientId: '', secret: '', url: null, linkShown: false, error: null };
}

// O passo seguinte dentro de `steps`, ou o fim.
function after(state) {
  const steps = stepsFor(state.loginOnly);
  return steps[steps.indexOf(state.step) + 1] ?? 'end';
}

// O passo de `Voltar`, ou `null` sem volta (depois do login não se volta para ele).
function previous(step) {
  if (step === 'waiting') return 'sign-in';
  const i = TOUR.indexOf(step);
  return i > 0 ? TOUR[i - 1] : null;
}

// Os botões do passo: `skip` e `back` aparecem ou não; `primary` é o rótulo do botão principal
// (`continue`, `retry`, `finish`) ou `null` (na espera, quem avança é o fim do login).
export function actions(state) {
  const last = after(state) === 'end';
  let primary = 'continue';
  if (state.step === 'waiting') primary = null;
  else if (last) primary = 'finish';
  else if (state.error?.[0] === 'onboarding-login-failed') primary = 'retry';
  return { skip: state.step !== 'waiting' && !last, back: previous(state.step) !== null, primary };
}

// O estado depois de `event`: `continue`, `back`, `skip`, `edit` (`field`, `value`),
// `login-url` (`url`), `show-link` ou `login-finished` (`ok`, `error`, `mount`).
export function nextStep(state, event) {
  // Trocar de passo esquece o erro e o link de login.
  const go = (step, changes = {}) => ({ ...state, step, error: null, url: null, linkShown: false, ...changes });
  switch (event.type) {
    case 'edit':
      return { ...state, [event.field]: event.value, error: null };
    case 'continue':
      if (state.step === 'sign-in' && (!state.clientId.trim() || !state.secret.trim())) {
        return { ...state, error: ['onboarding-missing-client'] };
      }
      return actions(state).primary ? go(after(state)) : state;
    case 'back': {
      const step = previous(state.step);
      return step ? go(step) : state;
    }
    case 'skip':
      return go('end');
    case 'login-url':
      return state.step === 'waiting' ? { ...state, url: event.url } : state;
    case 'show-link':
      return state.url ? { ...state, linkShown: true } : state;
    case 'login-finished':
      if (state.step !== 'waiting') return state;
      if (event.ok) return go(after(state), { mount: event.mount ?? state.mount });
      return go('sign-in', { error: ['onboarding-login-failed', { error: event.error }] });
    default:
      return state;
  }
}
