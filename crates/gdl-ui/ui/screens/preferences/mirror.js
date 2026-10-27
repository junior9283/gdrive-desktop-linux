// O modo do Drive na aba "Google Drive" sem DOM: Stream (a montagem) ou Espelho (uma cópia de
// tudo numa pasta do computador).

import { refusalOf } from './folders.js';

// A pasta sugerida para o Espelho: ao lado da montagem, com o nome do Meu Drive.
export function suggestedMirror(mount, myDrive) {
  const parent = mount.replace(/\/+$/, '').split('/').slice(0, -1).join('/');
  return parent ? `${parent}/${myDrive}` : '';
}

// O que a confirmação mostra da conferência do serviço (`MirrorCheck`): a recusa (chave do texto)
// ou o tamanho a baixar, o espaço livre, se cabe e o que ainda não foi enviado; `ok` libera o
// botão.
export function mirrorConfirm({ refusal, size, free, pending }) {
  const key = refusalOf(refusal);
  if (key) return { refusal: key, ok: false };
  const fits = size <= free;
  return { refusal: null, size, free, fits, pending, ok: fits };
}
