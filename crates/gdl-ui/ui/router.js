// Cada janela mostra a tela da query `?screen=` (o Rust só abre janelas das telas de
// `screens.rs`); sem query, a principal.

export function currentScreen(search) {
  return new URLSearchParams(search).get('screen') ?? 'home';
}

// O argumento da tela (`?arg=`), como o texto da pesquisa; `null` sem argumento.
export function screenArg(search) {
  return new URLSearchParams(search).get('arg');
}

// O argumento das telas de um item (`conta/id`, e em Compartilhar `conta/id/e-mail` com o e-mail
// já digitado): `{ account, id, email }`, com `id` nulo sem argumento. Sem a barra é só o id (a
// primeira conta).
export function itemArg(search) {
  const arg = screenArg(search);
  if (!arg) return { account: '', id: null, email: '' };
  const [first, id, email = ''] = arg.split('/');
  return id === undefined ? { account: '', id: first, email } : { account: first, id, email };
}
