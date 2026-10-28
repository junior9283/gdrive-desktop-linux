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

// O aviso abaixo da lista: só com a versão atual, não há versões anteriores.
export const versionsNote = (versions) => (versions.filter((v) => !v.current).length ? null : 'versions-none');
