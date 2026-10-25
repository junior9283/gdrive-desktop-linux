// A lógica da pesquisa rápida sem DOM: a espera entre teclas, o ícone de cada resultado e a
// navegação pelas setas.

// `fn` chamada `ms` depois da última chamada, com os argumentos dela; `.cancel()` desiste.
export function debounce(fn, ms) {
  let timer = null;
  const call = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
  call.cancel = () => clearTimeout(timer);
  return call;
}

const GOOGLE = {
  'application/vnd.google-apps.folder': 'folder',
  'application/vnd.google-apps.document': 'description',
  'application/vnd.google-apps.spreadsheet': 'table_chart',
  'application/vnd.google-apps.presentation': 'slideshow',
  'application/vnd.google-apps.form': 'list_alt',
  'application/vnd.google-apps.drawing': 'draw',
  'application/vnd.google-apps.shortcut': 'drive_file_move',
};

const EXTENSIONS = {
  picture_as_pdf: ['pdf'],
  image: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'heic', 'bmp'],
  movie: ['mp4', 'mkv', 'mov', 'avi', 'webm'],
  audio_file: ['mp3', 'ogg', 'flac', 'wav', 'm4a', 'opus'],
  folder_zip: ['zip', 'rar', '7z', 'tar', 'gz', 'xz'],
};
const BY_EXTENSION = new Map(Object.entries(EXTENSIONS).flatMap(([icon, list]) => list.map((ext) => [ext, icon])));

// O símbolo do resultado: pelo tipo do Google (pastas, Documentos Google, atalhos) ou, nos
// demais arquivos (que chegam sem tipo), pela extensão do nome.
export function iconFor(mime, name) {
  if (GOOGLE[mime]) return GOOGLE[mime];
  const dot = name.lastIndexOf('.');
  const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
  return BY_EXTENSION.get(ext) ?? 'draft';
}

// A linha escolhida depois de uma seta (`delta` ±1) numa lista de `count` linhas; `-1` é
// nenhuma. Para nas pontas.
export function moveSelection(index, delta, count) {
  if (!count) return -1;
  return Math.min(Math.max(index + delta, 0), count - 1);
}

// A pasta de `path` (caminho completo na montagem `mount`) como o usuário a vê no Drive: sem a
// montagem e sem o nome do item.
export function folderOf(path, mount) {
  const inside = path.startsWith(`${mount}/`) ? path.slice(mount.length + 1) : path;
  return inside.slice(0, Math.max(0, inside.lastIndexOf('/')));
}
