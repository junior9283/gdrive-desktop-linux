import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generalKinds, parseEmails } from '../screens/share/model.js';

test('e-mails separados por vírgula, ponto e vírgula ou espaço, sem repetir', () => {
  assert.deepEqual(parseEmails(' ana@x.com, Bia@y.com;ana@x.com\ncaio@z.com  '), {
    emails: ['ana@x.com', 'Bia@y.com', 'caio@z.com'],
    invalid: [],
  });
});

test('o que não parece e-mail é apontado', () => {
  assert.deepEqual(parseEmails('ana@x.com, joao, @x.com, bia@'), {
    emails: ['ana@x.com'],
    invalid: ['joao', '@x.com', 'bia@'],
  });
  assert.deepEqual(parseEmails('   '), { emails: [], invalid: [] });
});

test('o acesso de domínio só aparece em contas com domínio', () => {
  assert.deepEqual(generalKinds(''), ['restricted', 'anyone']);
  assert.deepEqual(generalKinds('empresa.com'), ['restricted', 'domain', 'anyone']);
});
