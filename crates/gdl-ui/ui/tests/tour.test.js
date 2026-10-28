import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { parseFtl } from '../i18n.js';
import { HOME_TOUR, onScreen, visibleSteps } from '../tour/model.js';

test('o tour pula os passos cujo alvo não está na tela', () => {
  const shown = new Set(['.header', '.more-button']);
  const steps = visibleSteps(HOME_TOUR, (selector) => shown.has(selector));
  assert.deepEqual(steps.map((s) => s.target), ['.header', '.more-button']);
  assert.deepEqual(visibleSteps(HOME_TOUR, () => false), []);
});

// Sem conta, a navegação existe mas fica escondida (`hidden`): sem caixa na tela, o tour a pula.
test('um alvo escondido conta como fora da tela', () => {
  const box = (rects) => ({ getClientRects: () => rects });
  assert.equal(onScreen(null), false);
  assert.equal(onScreen(box([])), false);
  assert.equal(onScreen(box([{ width: 10, height: 10 }])), true);
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
