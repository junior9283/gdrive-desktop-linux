#!/bin/sh
# Assina o <pasta>/SHA256SUMS com a chave do projeto (gera SHA256SUMS.minisig): o app só instala
# sozinho uma versão cujo SHA256SUMS confere com ela (RELEASE_PUBLIC_KEY). MINISIGN_SECRET_KEY é o
# conteúdo do .key (segredo do repositório); MINISIGN_PASSWORD, a senha dele, se tiver.
#
#   scripts/sign-sums.sh dist
set -eu
dir=${1:?uso: scripts/sign-sums.sh <pasta>}
: "${MINISIGN_SECRET_KEY:?falta o segredo MINISIGN_SECRET_KEY}"
[ -f "$dir/SHA256SUMS" ] || { echo "falta $dir/SHA256SUMS" >&2; exit 1; }
umask 077
key=$(mktemp)
trap 'rm -f "$key"' EXIT
printf '%s\n' "$MINISIGN_SECRET_KEY" > "$key"
# Sem terminal, o minisign lê a senha da entrada (vazia para uma chave sem senha).
printf '%s\n' "${MINISIGN_PASSWORD:-}" |
  minisign -S -s "$key" -m "$dir/SHA256SUMS" -x "$dir/SHA256SUMS.minisig" >/dev/null
echo "assinado: $dir/SHA256SUMS.minisig"
