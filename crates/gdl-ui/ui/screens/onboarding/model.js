// A lógica do primeiro uso sem DOM: em que passo está, para onde cada botão (ou o fim do login)
// leva e quais botões aparecem. Os erros saem como `[chave, argumentos]` do `ui.ftl`.

// O login começa pelo guia do Client ID, que quem já tem um passa com Continuar.
const LOGIN = ['setup', 'sign-in', 'waiting'];

// O guia do Client ID (base: `docs/guia-client-id.md`): cada parte abre a sua página do Google
// Cloud pelo nome do link (`gdl_common::links`); `copy` são os valores para colar lá, com o campo
// (`label`) onde vão.
const SITE = 'https://junior9283.github.io/gdrive-desktop-linux/';
export const GUIDE = [
  { id: 'project', link: 'gcp-new-project' },
  { id: 'drive-api', link: 'gcp-drive-api' },
  {
    // Sem página inicial, política e domínio autorizado, o Google não deixa publicar o app. As
    // páginas do site do projeto (GitHub Pages) servem para qualquer pessoa.
    id: 'branding',
    link: 'gcp-branding',
    copy: [
      { label: 'homepage', text: SITE },
      { label: 'privacy', text: `${SITE}politica-de-privacidade.html` },
      { label: 'terms', text: `${SITE}termos-de-uso.html` },
      { label: 'domain', text: new URL(SITE).host },
    ],
  },
  {
    id: 'scopes',
    link: 'gcp-scopes',
    copy: ['https://www.googleapis.com/auth/drive', 'openid', 'email'].map((text) => ({ text })),
  },
  { id: 'audience', link: 'gcp-audience' },
  { id: 'client', link: 'gcp-new-client' },
];

// Os problemas mais comuns do login, com o que fazer (os mesmos do fim do guia em `docs/`).
export const TROUBLES = ['publish-blocked', 'access-denied', 'redirect-mismatch', 'weekly-logout', 'api-disabled'];
// Depois do login, na ordem do original: apresentação, a pasta do Drive, pastas do computador,
// o resumo das escolhas e como deixar arquivos off-line.
const TOUR = ['intro', 'files', 'folders', 'summary', 'offline'];

// Os passos mostrados: com `DisableOnboardingDialog`, só os do login; adicionando outra conta
// (`mode` `add`) ou entrando de novo numa (`resume`), só o passo dela e a espera, sem o guia: as
// credenciais já guardadas valem. Com `DisableMirroredFolders` (`foldersOff`), sem as pastas.
export function stepsFor(loginOnly, mode = '', foldersOff = false) {
  if (mode) return [mode, 'waiting'];
  if (loginOnly) return [...LOGIN];
  return [...LOGIN, ...TOUR.filter((step) => !foldersOff || step !== 'folders')];
}

// Onde começa: nos modos `add` e `resume`, no passo deles (`expected` é o e-mail da conta que
// entra de novo); sem conta, no login; com a conta conectada, na apresentação (ou no fim, se ela
// está desligada). `mount` é a pasta do Drive.
export function firstState({ signedIn, loginOnly, mount = '', mode = '', expected = '', foldersOff = false }) {
  const step = mode || (!signedIn ? 'setup' : loginOnly ? 'end' : 'intro');
  return {
    step, loginOnly, foldersOff, mode, expected, mount, clientId: '', secret: '', url: null, linkShown: false, error: null,
    failed: false,
  };
}

// Os motivos de recusa do serviço que têm texto próprio (`onboarding-login-failed-<motivo>`).
export const REASONS = ['already-connected', 'other-account', 'too-many-accounts', 'add-account-disabled'];

// O erro de um login que não deu certo, como `[chave, argumentos]`.
export function loginFailure(error) {
  return REASONS.includes(error)
    ? [`onboarding-login-failed-${error}`]
    : ['onboarding-login-failed', { error }];
}

// As linhas do passo Resumo, como `[rótulo, valor]`; o valor é texto ou `[chave]` traduzível.
export function summaryRows({ email, mirror, mount, folders }) {
  return [
    ['onboarding-summary-account', email],
    ['onboarding-summary-mode', [mirror ? 'onboarding-summary-mirror' : 'onboarding-summary-stream']],
    ['onboarding-summary-mount', mount],
    ['onboarding-summary-folders', folders.length ? folders.join('\n') : ['onboarding-summary-no-folders']],
  ];
}

// O passo seguinte dentro de `steps`, ou o fim.
function after(state) {
  const steps = stepsFor(state.loginOnly, state.mode, state.foldersOff);
  return steps[steps.indexOf(state.step) + 1] ?? 'end';
}

// O passo de `Voltar`, ou `null` sem volta (depois do login não se volta para ele).
function previous({ step, mode }) {
  for (const steps of [mode ? [mode, 'waiting'] : LOGIN, TOUR]) {
    const i = steps.indexOf(step);
    if (i > 0) return steps[i - 1];
  }
  return null;
}

// Os botões do passo: `skip` e `back` aparecem ou não; `primary` é o rótulo do botão principal
// (`continue`, `retry`, `finish`) ou `null` (na espera, quem avança é o fim do login).
export function actions(state) {
  const last = after(state) === 'end';
  let primary = 'continue';
  if (state.step === 'waiting') primary = null;
  else if (last) primary = 'finish';
  else if (state.failed) primary = 'retry';
  return { skip: state.step !== 'waiting' && !last && !state.mode, back: previous(state) !== null, primary };
}

// O estado depois de `event`: `continue`, `back`, `skip`, `edit` (`field`, `value`),
// `login-url` (`url`), `show-link` ou `login-finished` (`ok`, `error`, `mount`).
export function nextStep(state, event) {
  // Trocar de passo esquece o erro e o link de login.
  const go = (step, changes = {}) => ({
    ...state, step, error: null, failed: false, url: null, linkShown: false, ...changes,
  });
  switch (event.type) {
    case 'edit':
      return { ...state, [event.field]: event.value, error: null, failed: false };
    case 'continue':
      if (state.step === 'sign-in' && (!state.clientId.trim() || !state.secret.trim())) {
        return { ...state, error: ['onboarding-missing-client'] };
      }
      return actions(state).primary ? go(after(state)) : state;
    case 'back': {
      const step = previous(state);
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
      return go(state.mode || 'sign-in', { error: loginFailure(event.error), failed: true });
    default:
      return state;
  }
}
