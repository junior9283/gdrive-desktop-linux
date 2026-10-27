// A aba "Meu computador" sem DOM: as linhas das pastas do computador e o que o configurador
// mostra de uma pasta escolhida.

// Os motivos de recusa que têm texto próprio no `ui.ftl` (`folder-refused-<motivo>`).
const REFUSALS = new Set(['missing', 'unreadable', 'nested', 'contains', 'network', 'folders-disabled']);

// A pasta do argumento da janela `folder:<caminho>` (o Dolphin e os avisos abrem assim), ou `null`.
export function folderArg(arg) {
  return arg?.startsWith('folder:') ? arg.slice('folder:'.length) : null;
}

export function folderName(path) {
  return path.replace(/\/+$/, '').split('/').pop() || path;
}

// As pastas do serviço (`LocalFolders`) em ordem de nome; `accountLabel` só com mais de uma conta
// e `notice` (`[chave, argumentos]`) só quando a pasta não está sincronizando.
export function folderRows(folders, manyAccounts) {
  return folders
    .map((f) => ({
      account: f.account,
      id: f.id,
      name: folderName(f.path),
      path: f.path,
      accountLabel: manyAccounts ? f.account : null,
      notice: notice(f),
      needsConfirm: f.state === 'confirm-deletes',
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.path.localeCompare(b.path));
}

function notice({ state, deletes }) {
  if (state === 'active') return null;
  return [`folder-state-${state}`, state === 'confirm-deletes' ? { count: deletes } : {}];
}

// O configurador para a conferência do serviço (`CheckLocalFolder`).
export function configurator({ refusal, size, files, exists }) {
  return { refusal: refusal ? refusalKey(refusal) ?? 'folder-refused-unreadable' : null, size, files, askMerge: exists };
}

// A chave do texto de um motivo de recusa do serviço, ou `null` quando não tem texto próprio.
export function refusalKey(reason) {
  return REFUSALS.has(reason) ? `folder-refused-${reason}` : null;
}
