#!/bin/sh
# Lista o que a instalação de uma variante põe no pacote: uma linha `sha256  /caminho` por arquivo,
# em ordem. Os programas são de mentira (sempre o mesmo conteúdo): entram pelo caminho.
#
#   scripts/stage-manifest.sh [--flavor VARIANTE] <saída>
#
# As variantes aceitas são as do install-dev.sh.
#
# O teste default_stage_matches_golden (crates/gdl-common/tests/packaging.rs) compara a variante
# atual com crates/gdl-common/tests/golden/default-stage.sha256.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
flavor=default
while [ $# -gt 1 ]; do
  case "$1" in
    --flavor) flavor=$2; shift 2 ;;
    *) echo "argumento desconhecido: $1" >&2; exit 2 ;;
  esac
done
[ $# -eq 1 ] || { echo "uso: scripts/stage-manifest.sh [--flavor VARIANTE] <saída>" >&2; exit 2; }
out=$1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/bins"
# Os mesmos programas que o install-dev.sh copia (a lista é a dele).
for program in $(sed -n 's/^programs="\(.*\)"$/\1/p' "$repo/scripts/install-dev.sh"); do
  printf '#!/bin/sh\n' > "$work/bins/$program"
done
sh "$repo/scripts/install-dev.sh" --flavor "$flavor" --prefix /usr --destdir "$work/stage" \
  --from "$work/bins" --no-dolphin >/dev/null

(cd "$work/stage" && find . -type f -exec sha256sum {} +) | sed 's|  \./|  /|' |
  LC_ALL=C sort -k2 > "$out"
