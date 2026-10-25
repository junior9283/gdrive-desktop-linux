// Cada janela mostra a tela da query `?screen=` (o Rust só abre janelas das telas de
// `screens.rs`); sem query, a principal.

export function currentScreen(search) {
  return new URLSearchParams(search).get('screen') ?? 'home';
}

// O argumento da tela (`?arg=`), como o texto da pesquisa; `null` sem argumento.
export function screenArg(search) {
  return new URLSearchParams(search).get('arg');
}
