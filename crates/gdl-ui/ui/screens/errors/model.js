// A lógica da lista de erros sem DOM: os erros do serviço agrupados por categoria.

// As categorias do `SyncErrorItem`, na ordem em que a tela mostra os grupos.
export const CATEGORIES = [
  'quota', 'upload-limit', 'download-limit', 'folder-too-deep', 'shared-drive-full', 'no-permission', 'too-large', 'cloud-missing', 'local-missing', 'lost-and-found', 'unknown',
];

// `[{ category, items }]` na ordem de `CATEGORIES`, sem grupos vazios; uma categoria que a tela
// não conhece entra em `unknown`. Dentro do grupo, a ordem do serviço.
export function groupErrors(errors) {
  const groups = new Map(CATEGORIES.map((category) => [category, []]));
  for (const error of errors) (groups.get(error.category) ?? groups.get('unknown')).push(error);
  return [...groups].filter(([, items]) => items.length).map(([category, items]) => ({ category, items }));
}
