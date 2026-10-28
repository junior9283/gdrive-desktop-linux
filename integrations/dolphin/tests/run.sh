#!/bin/sh
# Testes do plugin do Dolphin, num barramento D-Bus próprio e sem tela.
# Uso: run.sh PASTA_DO_BUILD (a do cmake, com o plugin e o probe compilados).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
build=$(cd "$1" && pwd)
if [ -z "${GDL_PRIVATE_BUS:-}" ]; then
  GDL_PRIVATE_BUS=1 exec dbus-run-session --config-file="$here/../../../scripts/e2e-bus.conf" -- "$0" "$@"
fi
export QT_QPA_PLATFORM=offscreen
# O KIO procura o plugin em <pasta>/kf6/kfileitemaction, como no sistema.
plugin=$(find "$build" -name gdrive_desktop_linux_menu.so | head -1)
export QT_PLUGIN_PATH="${plugin%/kf6/kfileitemaction/*}"
probe=$(find "$build" -name gdl-dolphin-probe -type f | head -1)
overlay=$(find "$build" -name gdl-overlay-probe -type f | head -1)
work=$(mktemp -d) fake=""
d="$work/drive"
pair="$work/Documentos"
mkdir -p "$d/pasta" "$pair" && touch "$d/a.txt" "$pair/c.txt"
trap 'kill "$fake" 2>/dev/null; rm -rf "$work"' EXIT
failed=0

expect() { # NOME ESPERADO OBTIDO
  if [ "$2" = "$3" ]; then
    echo "ok - $1"
  else
    echo "FALHOU - $1"; echo "esperado:"; echo "$2"; echo "obtido:"; echo "$3"
    failed=1
  fi
}

expect "sem o serviço, nada aparece" "(sem menu)" "$("$probe" "$d/a.txt")"

python3 -I -W ignore::DeprecationWarning "$here/../../testing/fake_service.py" "$work/actions" "$d" "$pair" >"$work/ready" &
fake=$!
until grep -q pronto "$work/ready" 2>/dev/null; do sleep 0.1; done

# Um item só: o menu inteiro que o serviço falso dá.
single="Google Drive
  Abrir no navegador
  Copiar link
  ---
  [x] Disponível off-line
  [ ] Somente on-line
  ---
  Ver itens na lixeira"
expect "um arquivo do Drive" "$single" "$("$probe" "$d/a.txt")"
expect "uma pasta do Drive" "$single" "$("$probe" "$d/pasta")"
expect "dois itens, sem links" "Google Drive
  [x] Disponível off-line
  [ ] Somente on-line
  ---
  Ver itens na lixeira" "$("$probe" "$d/a.txt" "$d/pasta")"
expect "fora do Drive, nada aparece" "(sem menu)" "$("$probe" "$d/a.txt" /home/x.txt)"
expect "copiar link vai para a área de transferência" "copiado: https://drive.google.com/file/d/x/view" \
  "$("$probe" --trigger "Copiar link" "$d/a.txt" | tail -1)"
expect "erro do serviço vira aviso" "erro: sem internet" \
  "$("$probe" --trigger "Somente on-line" "$d/a.txt" "$d/pasta" | tail -1)"
"$probe" --trigger "Ver itens na lixeira" "$d/pasta" >/dev/null
expect "as ações chegam ao serviço com os caminhos" "copy-link a.txt
online-only a.txt pasta
trash pasta" "$(cat "$work/actions")"
# Os emblemas: o atributo que a montagem dá a cada item; fora dela (sem atributo), nenhum.
mkdir -p "$work/fora" && touch "$work/fora/b.txt"
state() { python3 -I -c 'import os, sys; os.setxattr(sys.argv[1], "user.gdl.state", sys.argv[2].encode())' "$@"; }
state "$d/a.txt" offline
state "$d/pasta" cloud
touch "$d/falhou.txt" && state "$d/falhou.txt" error
expect "um emblema para cada estado" "a.txt: gdl-emblem-offline
pasta: gdl-emblem-cloud
falhou.txt: gdl-emblem-error
b.txt: " "$("$overlay" "$d/a.txt" "$d/pasta" "$d/falhou.txt" "$work/fora/b.txt")"
expect "pasta sincronizada: o estado vem do serviço, sem esperar" "Documentos: 
c.txt: 
mudou Documentos: gdl-emblem-offline
mudou c.txt: gdl-emblem-syncing" "$("$overlay" --wait 2 "$pair" "$pair/c.txt" | grep -v esperando)"
"$overlay" --wait 1 "$d/a.txt" > "$work/overlay" &
waiting=$!
until grep -q esperando "$work/overlay" 2>/dev/null; do sleep 0.1; done
state "$d/a.txt" syncing
python3 -I -W ignore::DeprecationWarning -c '
import sys
from gi.repository import Gio, GLib
name = "io.github.junior9283.GDriveDesktopLinux1"
Gio.bus_get_sync(Gio.BusType.SESSION).call_sync(name, "/io/github/junior9283/GDriveDesktopLinux1",
    name + ".Service", "EmitStates", GLib.Variant("(as)", (sys.argv[1:],)), None, 0, -1)' "$d/a.txt"
wait "$waiting" || true
expect "StatesChanged redesenha o emblema" "mudou a.txt: gdl-emblem-syncing" "$(tail -1 "$work/overlay")"
exit $failed
