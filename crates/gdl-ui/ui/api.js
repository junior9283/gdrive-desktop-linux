// A conversa da tela com o processo da janela. `call` vai ao serviço pela ponte (`bridge.rs`); o
// resto é local. Com o serviço fora do ar, avisa quem pediu (`onService`) e volta sozinho com o
// evento `service-up`.

import { currentAccount } from './state.js';

// `getAccount()` é a conta mostrada (vazio, a primeira): toda chamada a leva, salvo se quem
// chamou disse outra. A ponte (`bridge.rs`) só lê o `account` dos métodos que agem numa conta.
export function createApi(tauri, getAccount = () => '') {
  let onChange = () => {};
  const invoke = (cmd, args) => tauri().invoke(cmd, args);

  function setUp(up) {
    if (api.serviceUp === up) return;
    api.serviceUp = up;
    onChange(up);
  }

  const api = {
    serviceUp: true,

    async call(method, args = {}) {
      try {
        const account = getAccount();
        const withAccount = account ? { account, ...args } : args;
        const reply = await invoke('call', { method, args: withAccount });
        // Quem responde está no ar, mesmo que o `service-up` da ponte nunca tenha chegado.
        setUp(true);
        return reply;
      } catch (error) {
        if (error?.name === 'Unavailable') setUp(false);
        throw error;
      }
    },

    // `handler(payload)` a cada evento `event` (os sinais do serviço e `service-up`).
    on(event, handler) {
      const id = tauri().transformCallback((e) => handler(e.payload));
      return invoke('plugin:event|listen', { event, target: { kind: 'Any' }, handler: id });
    },

    // `handler(up)` quando o serviço cai ou volta (um só: o `app.js`).
    onService(handler) {
      onChange = handler;
      return Promise.all([api.on('service-up', () => setUp(true)), api.on('service-down', () => setUp(false))]);
    },

    strings: () => invoke('strings'),
    version: () => invoke('plugin:app|version'),
    // As bibliotecas que vão no app, `[{ name, version, license }]` (geradas no build).
    thirdParty: async () => JSON.parse(await invoke('third_party')),
    copy: (text) => invoke('clipboard_write', { text }),
    closeWindow: () => invoke('close_window'),
    openScreen: (screen, arg = null) => invoke('open_screen', { screen, arg }),
    // Um endereço da tabela fixa do Rust (`drive-web`, `storage`, `privacy`…).
    openLink: (name) => invoke('open_link', { name }),
    // A pasta do Drive no gerenciador de arquivos.
    openMount: () => invoke('open_mount'),
    // Uma pasta do computador sincronizada (pelo id; o caminho vem do serviço).
    openLocalFolder: (account, id) => invoke('open_local_folder', { account, id }),
    // As pastas do usuário que o primeiro uso sugere sincronizar (as que existem).
    suggestedFolders: () => invoke('suggested_folders'),
    // Uma pasta escolhida no seletor do sistema, ou `null`.
    pickFolder: (title) => invoke('pick_folder', { title }),
    // Tira o app do computador (a CLI pede a senha de administrador se ele veio de um pacote);
    // `purge` apaga também os dados locais. Falha com a mensagem da CLI.
    uninstall: (purge) => invoke('uninstall', { purge }),
  };
  return api;
}

// O valor de `key` na lista de `preferences` (os valores vêm em JSON), ou `fallback`.
export function prefValue(prefs, key, fallback = null) {
  const pref = prefs.find((p) => p.key === key);
  return pref ? JSON.parse(pref.value) ?? fallback : fallback;
}

export const api = createApi(() => globalThis.__TAURI_INTERNALS__, currentAccount);
