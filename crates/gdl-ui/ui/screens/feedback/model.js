// "Enviar feedback": o título e o texto da issue. A versão e o sistema entram no Rust, que monta
// o endereço (só o da página de issues do projeto) e o corta no limite do GitHub.

const TITLE_MAX = 80;

export const canSend = (text) => text.trim().length > 0;

// `kind`: `problem` ou `idea`; `diagnostics`: o pacote gerado, ou vazio. Do pacote vai só o nome:
// o caminho tem o nome do usuário.
export function feedbackIssue({ kind, text, diagnostics }, t) {
  const body = text.trim();
  const first = body.split('\n')[0].trim();
  const summary = first.length > TITLE_MAX ? `${first.slice(0, TITLE_MAX - 1)}…` : first;
  const attach = diagnostics ? `\n\n${t('feedback-attach', { file: diagnostics.split('/').at(-1) })}` : '';
  return { title: `${t(`feedback-kind-${kind}`)}: ${summary}`, body: body + attach };
}
