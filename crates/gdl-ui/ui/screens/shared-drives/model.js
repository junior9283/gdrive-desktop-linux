// A lógica da tela Drives compartilhados sem DOM: o argumento, as chaves e o salvar.

// O argumento `conta` ou `conta/new` (o menu da pasta "Novo Drive compartilhado" abre já com o
// diálogo de criar). E-mail não tem barra.
export function parseArg(arg) {
  const text = arg ?? '';
  const slash = text.indexOf('/');
  if (slash < 0) return { account: text, create: false };
  return { account: text.slice(0, slash), create: text.slice(slash + 1) === 'new' };
}

// A conta da tela: a do argumento; sem ela (a bandeja não sabe qual), a mostrada na janela, se
// ainda está conectada, ou a primeira.
export function shownAccount(arg, shown, accounts) {
  if (arg) return arg;
  return accounts.some((a) => a.email === shown) ? shown : accounts[0]?.email ?? '';
}

// A chave de um drive: o que o usuário mexeu (`overrides`, id → ligado) ou, sem mexer, o estado do
// serviço.
export const isOn = (drive, overrides) => (drive.id in overrides ? overrides[drive.id] : drive.enabled);

// Alguma chave difere do que está ligado no serviço.
export const changed = (drives, overrides) => drives.some((d) => isOn(d, overrides) !== d.enabled);

// Os drives como ficam depois de salvar: o estado do serviço passa a ser o das chaves.
export const applied = (drives, overrides) => drives.map((d) => ({ ...d, enabled: isOn(d, overrides) }));

// Manda ao serviço a lista toda de drives ligados (na ordem da tela). Com envio pendente num dos
// que seriam desligados o serviço recusa e nada muda: devolve o motivo para a tela mostrar; os
// outros erros seguem para quem chamou.
export async function saveDrives(api, account, drives, overrides) {
  const enabled = drives.filter((d) => isOn(d, overrides)).map((d) => d.id);
  try {
    await api.call('set_shared_drives', { account, enabled });
    return { ok: true };
  } catch (error) {
    if (error?.name === 'PendingUploads') return { ok: false, message: error.message };
    throw error;
  }
}
