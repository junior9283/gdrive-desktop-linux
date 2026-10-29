// A lógica da lista de erros sem DOM: os erros do serviço agrupados por categoria.

// As categorias do `SyncErrorItem`, na ordem em que a tela mostra os grupos.
export const CATEGORIES = [
  'quota', 'upload-limit', 'download-limit', 'folder-too-deep', 'shared-drive-full', 'no-permission', 'local-no-permission', 'too-large', 'cloud-missing', 'local-missing', 'lost-and-found', 'unknown',
];

// `[{ category, items }]` na ordem de `CATEGORIES`, sem grupos vazios; uma categoria que a tela
// não conhece entra em `unknown`. Dentro do grupo, a ordem do serviço.
export function groupErrors(errors) {
  const groups = new Map(CATEGORIES.map((category) => [category, []]));
  for (const error of errors) (groups.get(error.category) ?? groups.get('unknown')).push(error);
  return [...groups].filter(([, items]) => items.length).map(([category, items]) => ({ category, items }));
}

// As outras contas (não `current`) que têm erros, para a faixa "Há N erros em <e-mail>".
export function otherAccountErrors(accounts = [], current) {
  return accounts
    .filter((a) => a.email !== current && a.errors > 0)
    .map(({ email, errors }) => ({ email, errors }));
}
