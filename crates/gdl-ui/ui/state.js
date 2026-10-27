// A conta que a janela mostra (o e-mail; vazio, a primeira). Fica no `localStorage`, que as
// janelas do app compartilham: trocar a conta numa delas vale para as outras abertas, e o evento
// `storage` avisa quem mostra. Uma conta que não existe mais é corrigida por quem lê o `Status`.

const KEY = 'account';

// `storage()` dá o `localStorage` e `events` é a janela (que recebe o `storage` das outras).
export function createAccountState(storage, events) {
  const listeners = new Set();
  const read = () => storage().getItem(KEY) ?? '';
  const notify = () => listeners.forEach((handler) => handler(read()));
  return {
    currentAccount: read,
    setCurrentAccount(email) {
      if (email === read()) return;
      storage().setItem(KEY, email);
      notify();
    },
    // `handler(email)` quando a conta mostrada muda, aqui ou em outra janela.
    onAccountChange(handler) {
      if (!listeners.size) events.addEventListener('storage', (e) => e.key === KEY && notify());
      listeners.add(handler);
    },
  };
}

export const { currentAccount, setCurrentAccount, onAccountChange } = createAccountState(
  () => globalThis.localStorage,
  globalThis,
);
