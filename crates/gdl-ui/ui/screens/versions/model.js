// A lógica da janela Gerenciar versões sem DOM: o que cada versão deixa fazer.

// As ações do menu de uma versão: todos baixam; quem edita restaura as antigas e marca (ou
// desmarca) para o Drive nunca apagar.
export function versionActions(version, canEdit) {
  if (!canEdit) return ['download'];
  return [
    'download',
    ...(version.current ? [] : ['restore']),
    version.keep_forever ? 'unkeep' : 'keep',
  ];
}
