import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canSend, feedbackIssue } from '../screens/feedback/model.js';

const t = (key, args) => (args ? `${key}${JSON.stringify(args)}` : key);

test('o título é o tipo e a primeira linha da descrição', () => {
  const issue = feedbackIssue({ kind: 'problem', text: '  A pasta não abre\nmais detalhes', diagnostics: '' }, t);
  assert.equal(issue.title, 'feedback-kind-problem: A pasta não abre');
  assert.equal(issue.body, 'A pasta não abre\nmais detalhes');
});

test('título longo é cortado', () => {
  const issue = feedbackIssue({ kind: 'idea', text: 'x'.repeat(200), diagnostics: '' }, t);
  assert.equal(issue.title, `feedback-kind-idea: ${'x'.repeat(79)}…`);
});

test('com o pacote, o corpo pede para anexar só o arquivo, sem o caminho', () => {
  const diagnostics = '/home/ana/Área de trabalho/gdrive-desktop-linux-diagnostico-2026.tar.gz';
  const issue = feedbackIssue({ kind: 'problem', text: 'Erro', diagnostics }, t);
  assert.equal(
    issue.body,
    'Erro\n\nfeedback-attach{"file":"gdrive-desktop-linux-diagnostico-2026.tar.gz"}',
  );
  assert.ok(!issue.body.includes('/home/ana'));
});

test('só envia com descrição', () => {
  assert.equal(canSend(' \n '), false);
  assert.equal(canSend('oi'), true);
});
