// Os dispositivos removíveis na aba "Meu computador": o estado de cada um e os botões que cabem.

const STATUS = { backup: 'pref-device-always', ignored: 'pref-device-ignored' };

// As linhas de `Devices`: conectado faz backup (se não estiver fazendo) e é ignorado (se ainda
// não for); uma escolha guardada pode ser removida, o que basta para os desconectados.
export function deviceRows(devices) {
  return devices.map(({ id, label, path, choice, busy }) => {
    const actions = [
      ...(path && !busy ? ['backup'] : []),
      ...(path && choice !== 'ignored' ? ['ignore'] : []),
      ...(choice ? ['forget'] : []),
    ];
    return { id, label, path, choice, status: busy ? 'pref-device-busy' : STATUS[choice] ?? null, actions };
  });
}
