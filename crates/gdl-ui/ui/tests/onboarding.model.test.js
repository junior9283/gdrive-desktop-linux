import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { GUIDE, actions, firstState, nextStep, stepsFor } from '../screens/onboarding/model.js';

const signIn = (fields = {}) => ({ ...firstState({ signedIn: false, loginOnly: false }), step: 'sign-in', ...fields });

test('Continuar no login sem Client ID não avança', () => {
  const state = nextStep(signIn({ secret: 'segredo' }), { type: 'continue' });
  assert.equal(state.step, 'sign-in');
  assert.deepEqual(state.error, ['onboarding-missing-client']);
});

test('Continuar com Client ID e segredo vai esperar o login', () => {
  const state = nextStep(signIn({ clientId: ' id ', secret: 'segredo', error: ['x'] }), { type: 'continue' });
  assert.equal(state.step, 'waiting');
  assert.equal(state.error, null);
});

test('login concluído vai para a apresentação; só login, para o fim', () => {
  const waiting = { ...signIn({ clientId: 'id', secret: 's' }), step: 'waiting' };
  assert.equal(nextStep(waiting, { type: 'login-finished', ok: true }).step, 'intro');
  assert.equal(nextStep({ ...waiting, loginOnly: true }, { type: 'login-finished', ok: true }).step, 'end');
});

test('erro no login volta ao primeiro passo com a mensagem', () => {
  const waiting = { ...signIn({ clientId: 'id', secret: 's' }), step: 'waiting', url: 'http://x' };
  const state = nextStep(waiting, { type: 'login-finished', ok: false, error: 'tempo esgotado' });
  assert.equal(state.step, 'sign-in');
  assert.deepEqual(state.error, ['onboarding-login-failed', { error: 'tempo esgotado' }]);
  assert.equal(state.url, null);
  assert.equal(state.clientId, 'id');
});

test('login-finished fora da espera não muda o passo', () => {
  assert.equal(nextStep(signIn(), { type: 'login-finished', ok: true }).step, 'sign-in');
});

test('Pular da apresentação vai ao fim', () => {
  assert.equal(nextStep({ ...signIn(), step: 'intro' }, { type: 'skip' }).step, 'end');
});

test('Continuar e Voltar percorrem os passos depois do login', () => {
  let state = { ...signIn(), step: 'intro' };
  state = nextStep(state, { type: 'continue' });
  assert.equal(state.step, 'files');
  state = nextStep(state, { type: 'continue' });
  assert.equal(state.step, 'offline');
  assert.equal(nextStep(state, { type: 'back' }).step, 'files');
  assert.equal(nextStep(state, { type: 'continue' }).step, 'end');
  // Depois do login não se volta para ele.
  assert.equal(nextStep({ ...state, step: 'intro' }, { type: 'back' }).step, 'intro');
  assert.equal(nextStep({ ...state, step: 'waiting' }, { type: 'back' }).step, 'sign-in');
});

test('com a conta já conectada começa na apresentação; só login, no fim', () => {
  assert.equal(firstState({ signedIn: true, loginOnly: false }).step, 'intro');
  assert.equal(firstState({ signedIn: true, loginOnly: true }).step, 'end');
  assert.equal(firstState({ signedIn: false, loginOnly: true }).step, 'setup');
});

test('sem conta começa pelo guia do Client ID, que leva ao login e volta dele', () => {
  const setup = firstState({ signedIn: false, loginOnly: false });
  assert.equal(setup.step, 'setup');
  const state = nextStep(setup, { type: 'continue' });
  assert.equal(state.step, 'sign-in');
  assert.equal(state.error, null, 'o guia não pede o Client ID');
  assert.equal(nextStep(state, { type: 'back' }).step, 'setup');
  assert.equal(nextStep(setup, { type: 'back' }).step, 'setup');
});

// Um arquivo do repositório, a partir da raiz.
const repo = (path) => readFileSync(new URL(`../../../../${path}`, import.meta.url), 'utf8');

test('o guia tem os passos do Google Cloud, cada um com um link da tabela fixa', () => {
  // Os textos citam as partes pelo número ("parte 5"): a ordem faz parte do contrato.
  assert.deepEqual(GUIDE.map((s) => s.id), ['project', 'drive-api', 'branding', 'scopes', 'audience', 'client']);
  const known = new Set([...repo('crates/gdl-common/src/links.rs').matchAll(/"(gcp-[a-z-]+)" =>/g)].map((m) => m[1]));
  for (const section of GUIDE) assert.ok(known.has(section.link), section.link);
});

test('os escopos para copiar são os que o login pede', () => {
  const [, scopes] = repo('crates/gdl-drive/src/oauth.rs').match(/const SCOPES: &str = "([^"]+)";/);
  const copy = GUIDE.find((s) => s.id === 'scopes').copy;
  assert.deepEqual([...copy].sort(), scopes.split(' ').sort());
});

test('DisableOnboardingDialog deixa só os passos do login', () => {
  assert.deepEqual(stepsFor(true), ['setup', 'sign-in', 'waiting']);
  assert.deepEqual(stepsFor(false), ['setup', 'sign-in', 'waiting', 'intro', 'files', 'offline']);
});

test('editar um campo guarda o valor e limpa o erro', () => {
  const state = nextStep(signIn({ error: ['onboarding-missing-client'] }), { type: 'edit', field: 'clientId', value: 'abc' });
  assert.equal(state.clientId, 'abc');
  assert.equal(state.error, null);
});

test('botões de cada passo', () => {
  const at = (step, extra = {}) => actions({ ...signIn(), step, ...extra });
  assert.deepEqual(at('setup'), { skip: true, back: false, primary: 'continue' });
  assert.deepEqual(at('sign-in'), { skip: true, back: true, primary: 'continue' });
  assert.deepEqual(at('sign-in', { error: ['onboarding-login-failed', { error: 'x' }] }).primary, 'retry');
  assert.deepEqual(at('waiting'), { skip: false, back: true, primary: null });
  assert.deepEqual(at('intro'), { skip: true, back: false, primary: 'continue' });
  assert.deepEqual(at('files'), { skip: true, back: true, primary: 'continue' });
  assert.deepEqual(at('offline'), { skip: false, back: true, primary: 'finish' });
  assert.deepEqual(actions({ ...signIn(), loginOnly: true }).primary, 'continue');
});

test('o link de login só vale na espera e some ao sair dela', () => {
  const waiting = { ...signIn({ clientId: 'id', secret: 's' }), step: 'waiting' };
  let state = nextStep(waiting, { type: 'login-url', url: 'http://login' });
  assert.equal(state.url, 'http://login');
  assert.equal(state.linkShown, false);
  state = nextStep(state, { type: 'show-link' });
  assert.equal(state.linkShown, true);
  const back = nextStep(state, { type: 'back' });
  assert.equal(back.url, null);
  assert.equal(back.linkShown, false);
  // Uma resposta atrasada, depois de sair da espera, não volta a mostrar o link.
  assert.equal(nextStep(back, { type: 'login-url', url: 'http://velho' }).url, null);
});

test('o login concluído traz a pasta do Drive', () => {
  const waiting = { ...signIn({ clientId: 'id', secret: 's' }), step: 'waiting' };
  assert.equal(nextStep(waiting, { type: 'login-finished', ok: true, mount: '/home/x/Google Drive' }).mount, '/home/x/Google Drive');
  assert.equal(firstState({ signedIn: true, loginOnly: false, mount: '/m' }).mount, '/m');
});
