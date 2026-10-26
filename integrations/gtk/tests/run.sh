#!/bin/sh
# Testes da extensão do Nautilus, do Nemo, do Caja e do Thunar, num barramento D-Bus próprio.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
exec dbus-run-session --config-file="$here/../../../scripts/e2e-bus.conf" -- \
  python3 -I -W ignore::DeprecationWarning "$here/test_menu.py" "$@"
