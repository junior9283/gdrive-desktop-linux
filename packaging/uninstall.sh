#!/bin/sh
# O uninstall.sh do .tar.gz do release: desinstala o que o install.sh pôs em ~/.local (ou em
# --prefix DIR). --purge apaga também os dados deste computador (desconecte a conta antes);
# --yes não pergunta.
set -eu
prefix="$HOME/.local" purge="" yes=""
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix=$2; shift 2 ;;
    --purge) purge=--purge; shift ;;
    --yes) yes=--yes; shift ;;
    *) echo "argumento desconhecido: $1" >&2; exit 2 ;;
  esac
done
exec "$prefix/bin/gdrive-desktop-linux" uninstall $purge $yes
