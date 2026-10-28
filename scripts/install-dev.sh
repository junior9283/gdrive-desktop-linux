#!/bin/sh
# Instala o app para uso, sem root, em <prefixo> (padrão ~/.local): os programas, o serviço do
# usuário (systemd), a ativação do serviço e da janela pelo D-Bus, o .desktop (menu e ponteiros
# dos Documentos Google), os tipos dos ponteiros, os ícones e o menu do botão direito nos
# gerenciadores de arquivos GTK.
#
#   scripts/install-dev.sh [--prefix DIR] [--destdir DIR] [--from DIR] [--dry-run] [--no-activate]
#
# --from usa os programas já compilados em DIR (sem ele, compila com `cargo build --release`);
# --dry-run só lista o que instalaria; --no-activate não avisa o systemd nem o D-Bus nem atualiza
# os bancos de tipos e de aplicativos; --destdir grava em DIR/<prefixo> os arquivos que vão funcionar
# em <prefixo> (para montar os pacotes; implica --no-activate).
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
prefix="$HOME/.local" destdir="" from="" dry=false activate=true
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix=$2; shift 2 ;;
    --destdir) destdir=$2; activate=false; shift 2 ;;
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
programs="gdrive-desktop-linuxd gdrive-desktop-linux gdrive-desktop-linux-tray gdrive-desktop-linux-ui gdrive-desktop-linux-mcp"

if [ -z "$from" ] && ! $dry; then
  (cd "$repo" && cargo build --release -p gdl-daemon -p gdl-cli -p gdl-tray -p gdl-ui -p gdl-mcp --bins)
fi
from=${from:-$repo/target/release}

# install_as <modo> <origem> <destino>: copia como está.
# O que foi instalado, um caminho por linha: a lista que a desinstalação apaga.
installed=""

install_as() {
  echo "instala $destdir$3"
  installed="$installed$3
"
  $dry || install -Dm"$1" "$2" "$destdir$3"
}

# remove <arquivo>: tira um arquivo de uma instalação anterior, se houver.
remove() {
  [ -e "$destdir$1" ] || return 0
  echo "remove $destdir$1"
  $dry || rm -f "$destdir$1"
}

# put <origem> <destino>: copia um arquivo com Exec, trocando /usr/bin/<programa> pelo programa
# instalado, entre aspas.
put() {
  echo "instala $destdir$2"
  installed="$installed$2
"
  $dry && return
  mkdir -p "$(dirname "$destdir$2")"
  sed "s|/usr/bin/\([a-z-]*\)|\"$bin/\1\"|g" "$1" > "$destdir$2.tmp"
  chmod 644 "$destdir$2.tmp"
  mv "$destdir$2.tmp" "$destdir$2"
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
# Os emblemas de estado do Dolphin, no tema do sistema (é nele que o Dolphin procura).
for file in "$repo"/assets/icons/hicolor/scalable/emblems/*.svg; do
  install_as 644 "$file" "$share/icons/hicolor/scalable/emblems/$(basename "$file")"
done
# Os ícones da bandeja, no tema próprio que ela procura ao lado dos programas.
for file in "$repo"/assets/icons/hicolor/scalable/status/*.svg; do
  install_as 644 "$file" "$share/gdrive-desktop-linux/icons/hicolor/scalable/status/$(basename "$file")"
done
# O menu do botão direito no Nautilus, no Nemo, no Caja e no Thunar: a mesma extensão na pasta de
# cada um (só carrega onde o pacote de Python do gerenciador estiver instalado).
for manager in nautilus nemo caja thunarx; do
  install_as 644 "$repo/integrations/gtk/gdrive_desktop_linux_menu.py" \
    "$share/$manager-python/extensions/gdrive_desktop_linux_menu.py"
done

# Os pacotes trazem um marcador do formato no lugar da lista (o gerenciador sabe o que é dele).
if ! $dry && [ -z "$destdir" ]; then
  printf '%s' "$installed" > "$share/gdrive-desktop-linux/installed-files"
fi

if $dry || ! $activate; then exit 0; fi
# A mesma lista que a desinstalação repete (`refresh` em crates/gdl-cli/src/uninstall.rs).
update-mime-database "$share/mime" >/dev/null 2>&1 || true
update-desktop-database "$share/applications" >/dev/null 2>&1 || true
gtk-update-icon-cache -q -t "$share/icons/hicolor" >/dev/null 2>&1 || true
systemctl --user daemon-reload || true
# Sem isto, o barramento da sessão só conhece a ativação nova na próxima sessão.
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig >/dev/null ||
  echo "aviso: o D-Bus não releu a configuração; se a janela disser que o serviço não responde, saia e entre de novo na sessão" >&2
echo "Pronto. Abra \"Google Drive\" no menu de aplicativos (ou rode $bin/gdrive-desktop-linux-tray)."
