#!/bin/sh
# Compila os plugins do Dolphin (menu e emblemas), roda os testes deles e copia os .so para <saída>.
#
#   integrations/dolphin/build.sh <saída>
#
# Precisa do CMake, do ECM, do Qt 6 e do KIO 6, e para os testes do PyGObject e do D-Bus (no
# Debian 13, o integrations/debian-deps.sh instala tudo).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT
cmake -S "$here" -B "$build" -DCMAKE_BUILD_TYPE=Release -Wno-dev
cmake --build "$build" -j"$(nproc)"
(cd "$build" && ctest --output-on-failure)
for plugin in gdrive_desktop_linux_menu gdrive_desktop_linux_overlay; do
  cp "$(find "$build" -name "$plugin.so" | head -1)" "$out/"
done
