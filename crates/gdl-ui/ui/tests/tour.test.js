import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parseFtl } from '../i18n.js';
import { HOME_TOUR, visibleSteps } from '../tour/model.js';

test('o tour pula os passos cujo alvo não está na tela', () => {
  const shown = new Set(['.header', '.more-button']);
  const steps = visibleSteps(HOME_TOUR, (selector) => shown.has(selector));
  assert.deepEqual(steps.map((s) => s.target), ['.header', '.more-button']);
  assert.deepEqual(visibleSteps(HOME_TOUR, () => false), []);
});

test('cada passo do tour tem texto nas duas línguas', () => {
  for (const lang of ['pt-BR', 'en']) {
    const keys = parseFtl(readFileSync(new URL(`../../../../locales/${lang}/ui.ftl`, import.meta.url), 'utf8'));
    for (const step of HOME_TOUR) {
      assert.ok(keys[step.title], `${lang}: ${step.title}`);
      assert.ok(keys[step.text], `${lang}: ${step.text}`);
    }
  }
});
