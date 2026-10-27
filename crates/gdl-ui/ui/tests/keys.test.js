import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseFtl } from '../i18n.js';
import { CATEGORIES } from '../screens/errors/model.js';
import { GUIDE, REASONS, TROUBLES, stepsFor } from '../screens/onboarding/model.js';
import { ROLES, generalKinds } from '../screens/share/model.js';

const ui = fileURLToPath(new URL('..', import.meta.url));
const locale = (lang) =>
  parseFtl(readFileSync(new URL(`../../../../locales/${lang}/ui.ftl`, import.meta.url), 'utf8'));

// Os arquivos .js das telas (fora os testes).
function sources(dir = ui) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return entry.name === 'tests' ? [] : sources(path);
    return entry.name.endsWith('.js') ? [path] : [];
  });
}

test('toda chave usada nas telas existe nos dois idiomas', () => {
  const used = new Set();
  for (const file of sources()) {
    for (const [, key] of readFileSync(file, 'utf8').matchAll(/\bt\('([a-z0-9-]+)'/g)) used.add(key);
  }
  // As montadas com o nome da visão ou do estado.
  for (const view of ['start', 'activity', 'offline', 'shared', 'notifications', 'links']) used.add(`view-${view}`);
  for (const state of [
    'upload-queued', 'uploading', 'uploaded', 'upload-canceling', 'upload-canceled', 'upload-failed',
    'downloading', 'downloaded', 'trashing', 'trashed', 'restoring', 'restored',
  ]) used.add(`activity-${state}`);
  for (const link of ['help', 'privacy', 'terms', 'faq', 'drive-web']) used.add(`link-${link}`);
  for (const step of [...stepsFor(false), ...stepsFor(false, 'add'), ...stepsFor(false, 'resume')]) used.add(`onboarding-step-${step}`);
  for (const reason of REASONS) used.add(`onboarding-login-failed-${reason}`);
  for (const { id, copy = [] } of GUIDE) {
    used.add(`onboarding-setup-${id}-title`).add(`onboarding-setup-${id}-text`);
    for (const { label } of copy) if (label) used.add(`onboarding-setup-field-${label}`);
  }
  for (const id of TROUBLES) used.add(`onboarding-trouble-${id}`).add(`onboarding-trouble-${id}-fix`);
  for (const category of CATEGORIES) used.add(`error-category-${category}`).add(`error-category-${category}-text`);
  for (const part of ['offline', 'cache', 'free']) used.add(`offline-usage-${part}`);
  for (const role of [...ROLES, 'owner', 'remove']) used.add(`share-role-${role}`);
  for (const kind of generalKinds('x')) used.add(`share-general-${kind}`).add(`share-general-${kind}-text`);
  for (const action of ['download', 'restore', 'keep', 'unkeep']) used.add(`versions-${action}`);
  for (const block of ['stream', 'sync', 'offline']) {
    used.add(`onboarding-intro-${block}-title`).add(`onboarding-intro-${block}-text`);
  }
  for (const lang of ['pt-BR', 'en']) {
    const messages = locale(lang);
    const missing = [...used].filter((key) => !(key in messages));
    assert.deepEqual(missing, [], lang);
  }
});
