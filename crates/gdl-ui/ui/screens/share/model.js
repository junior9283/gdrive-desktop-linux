// A lógica da janela Compartilhar sem DOM: os e-mails digitados e os tipos de acesso geral.

// Os papéis que a janela dá, do menor para o maior.
export const ROLES = ['reader', 'commenter', 'writer'];

// Algo antes e depois do `@`, com um ponto no domínio (o Drive confere o resto).
const EMAIL = /^[^@\s]+@[^@\s]+\.[^@\s]+$/;

// Os e-mails do campo (separados por vírgula, ponto e vírgula ou espaço), sem repetir, e os que
// não parecem e-mail.
export function parseEmails(text) {
  const words = [...new Set(text.split(/[\s,;]+/).filter(Boolean))];
  return {
    emails: words.filter((w) => EMAIL.test(w)),
    invalid: words.filter((w) => !EMAIL.test(w)),
  };
}

// As escolhas de acesso geral: o de domínio só em contas do Workspace (com domínio).
export function generalKinds(domain) {
  return domain ? ['restricted', 'domain', 'anyone'] : ['restricted', 'anyone'];
}
