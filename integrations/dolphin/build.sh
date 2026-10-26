#!/bin/sh
# Compila o plugin do Dolphin, roda os testes dele e copia o .so para <saída>.
#
#   integrations/dolphin/build.sh <saída>
#
# Precisa do CMake, do ECM, do Qt 6 e do KIO 6, e para os testes do PyGObject e do D-Bus (no
# Debian 13, o debian-deps.sh ao lado instala tudo).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$1"
out=$(cd "$1" && pwd)
build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT
cmake -S "$here" -B "$build" -DCMAKE_BUILD_TYPE=Release -Wno-dev
cmake --build "$build" -j"$(nproc)"
(cd "$build" && ctest --output-on-failure)
cp "$(find "$build" -name gdrive_desktop_linux_menu.so | head -1)" "$out/"
