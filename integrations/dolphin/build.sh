#!/bin/sh
# Compila os plugins do Dolphin (menu e emblemas), roda os testes deles e copia os .so para <saída>.
#
#   integrations/dolphin/build.sh [--flavor default|gerasync] <saída>
#
# --flavor gerasync compila os do GeraSync: os mesmos fontes passados pela tabela
# packaging/gerasync/rename.sed (o D-Bus, o menu, os emblemas e os nomes dos .so dele).
#
# Precisa do CMake, do ECM, do Qt 6 e do KIO 6, e para os testes do PyGObject e do D-Bus (no
# Debian 13, o integrations/debian-deps.sh instala tudo).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
flavor=default
if [ "${1:-}" = --flavor ]; then
  flavor=$2
  shift 2
fi
[ $# -eq 1 ] || { echo "uso: integrations/dolphin/build.sh [--flavor VARIANTE] <saída>" >&2; exit 2; }
mkdir -p "$1"
out=$(cd "$1" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
build="$work/build" src=$here
case "$flavor" in
  default) ;;
  gerasync)
    # Os fontes, os testes e o que os testes usam do repositório, nas mesmas posições.
    repo=$(cd "$here/../.." && pwd)
    mkdir -p "$work/repo/scripts"
    cp -rp "$repo/integrations" "$work/repo/"
    cp -p "$repo/scripts/e2e-bus.conf" "$work/repo/scripts/"
    find "$work/repo" -type f -exec sed -i -f "$repo/packaging/gerasync/rename.sed" {} +
    src="$work/repo/integrations/dolphin" ;;
  *) echo "variante desconhecida: $flavor" >&2; exit 2 ;;
esac
cmake -S "$src" -B "$build" -DCMAKE_BUILD_TYPE=Release -Wno-dev
cmake --build "$build" -j"$(nproc)"
(cd "$build" && ctest --output-on-failure)
for plugin in $(echo gdrive_desktop_linux_menu gdrive_desktop_linux_overlay | sed -f "$here/../../packaging/gerasync/rename.sed"); do
  cp "$(find "$build" -name "$plugin.so" | head -1)" "$out/"
done
