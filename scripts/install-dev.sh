#!/bin/sh
# Instala o app para uso, sem root, em <prefixo> (padrão ~/.local): os programas, o serviço do
# usuário (systemd), a ativação do serviço e da janela pelo D-Bus, o .desktop (menu e ponteiros
# dos Documentos Google), os tipos dos ponteiros e os ícones.
#
#   scripts/install-dev.sh [--prefix DIR] [--from DIR] [--dry-run] [--no-activate]
#
# --from usa os programas já compilados em DIR (sem ele, compila com `cargo build --release`);
# --dry-run só lista o que instalaria; --no-activate não avisa o systemd nem atualiza os bancos
# de tipos e de aplicativos.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
prefix="$HOME/.local" from="" dry=false activate=true
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix=$2; shift 2 ;;
    --from) from=$2; shift 2 ;;
    --dry-run) dry=true; shift ;;
    --no-activate) activate=false; shift ;;
    *) echo "argumento desconhecido: $1" >&2; exit 2 ;;
  esac
done
# O caminho vai entre aspas no Exec do systemd, do D-Bus e do .desktop, onde " \ ` $ % não cabem
# (a mesma lista do início automático, em crates/gdl-daemon/src/desktop.rs); | e & quebrariam o sed.
case "$prefix" in
  *[\"\\\$\`%\|\&]*) echo "prefixo com caractere que o Exec não aceita: $prefix" >&2; exit 2 ;;
  /*) ;;
  *) prefix="$PWD/$prefix" ;;
esac
bin="$prefix/bin" share="$prefix/share"
programs="gdrive-desktop-linuxd gdrive-desktop-linux gdrive-desktop-linux-tray gdrive-desktop-linux-ui"

if [ -z "$from" ] && ! $dry; then
  (cd "$repo" && cargo build --release -p gdl-daemon -p gdl-cli -p gdl-tray -p gdl-ui --bins)
fi
from=${from:-$repo/target/release}

# install_as <modo> <origem> <destino>: copia como está.
install_as() {
  echo "instala $3"
  $dry || install -Dm"$1" "$2" "$3"
}

# remove <arquivo>: tira um arquivo de uma instalação anterior, se houver.
remove() {
  [ -e "$1" ] || return 0
  echo "remove $1"
  $dry || rm -f "$1"
}

# put <origem> <destino>: copia um arquivo com Exec, trocando /usr/bin/<programa> pelo programa
# instalado, entre aspas.
put() {
  echo "instala $2"
  $dry && return
  mkdir -p "$(dirname "$2")"
  sed "s|/usr/bin/\([a-z-]*\)|\"$bin/\1\"|g" "$1" > "$2.tmp"
  chmod 644 "$2.tmp"
  mv "$2.tmp" "$2"
}

for program in $programs; do
  install_as 755 "$from/$program" "$bin/$program"
done
put "$repo/packaging/systemd/gdrive-desktop-linuxd.service" "$share/systemd/user/gdrive-desktop-linuxd.service"
for file in "$repo"/packaging/dbus/*.service; do
  put "$file" "$share/dbus-1/services/$(basename "$file")"
done
for file in "$repo"/packaging/desktop/*.desktop; do
  put "$file" "$share/applications/$(basename "$file")"
done
install_as 644 "$repo/packaging/mime/gdrive-desktop-linux.xml" "$share/mime/packages/gdrive-desktop-linux.xml"
# O ícone do app, nos tamanhos do tema (gerados por scripts/update-app-icon.py). O SVG de uma
# instalação anterior sai: o tema o preferiria aos PNG.
for file in "$repo"/assets/icons/hicolor/*/apps/*.png; do
  install_as 644 "$file" "$share/icons/hicolor/${file#"$repo/assets/icons/hicolor/"}"
done
remove "$share/icons/hicolor/scalable/apps/io.github.junior9283.GDriveDesktopLinux.svg"
# Os ícones da bandeja, no tema próprio que ela procura ao lado dos programas.
for file in "$repo"/assets/icons/hicolor/scalable/status/*.svg; do
  install_as 644 "$file" "$share/gdrive-desktop-linux/icons/hicolor/scalable/status/$(basename "$file")"
done

if $dry || ! $activate; then exit 0; fi
update-mime-database "$share/mime" >/dev/null 2>&1 || true
update-desktop-database "$share/applications" >/dev/null 2>&1 || true
gtk-update-icon-cache -q -t "$share/icons/hicolor" >/dev/null 2>&1 || true
systemctl --user daemon-reload || true
echo "Pronto. Abra \"Google Drive\" no menu de aplicativos (ou rode $bin/gdrive-desktop-linux-tray)."
