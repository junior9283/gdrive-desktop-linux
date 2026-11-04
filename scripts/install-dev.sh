#!/bin/sh
# Instala o app para uso, sem root, em <prefixo> (padrão ~/.local): os programas, o serviço do
# usuário (systemd), a ativação do serviço e da janela pelo D-Bus, o .desktop (menu e ponteiros
# dos Documentos Google), os tipos dos ponteiros, os ícones e o menu do botão direito nos
# gerenciadores de arquivos GTK e os plugins do Dolphin (menu e emblemas).
#
#   scripts/install-dev.sh [--flavor default|gerasync] [--prefix DIR] [--destdir DIR] [--from DIR]
#                          [--dry-run] [--no-activate] [--dolphin DIR | --no-dolphin]
#
# --flavor gerasync instala o GeraSync (o Client ID embutido no build): os mesmos arquivos, com os
# nomes e o conteúdo passados pela tabela packaging/gerasync/rename.sed, e o ícone dele. As duas
# variantes não dividem nenhum arquivo e podem ficar instaladas juntas.
#
# Os plugins do Dolphin vão em <prefixo>/lib/qt6/plugins, e o Qt só olha ali se o QT_PLUGIN_PATH
# disser; por isso o instalador grava um arquivo em ~/.config/environment.d (vale na próxima
# sessão). --dolphin usa os .so já compilados em DIR (o que o integrations/dolphin/build.sh
# preenche); sem ele, na instalação em ~/.local feita do código-fonte, o instalador compila os
# plugins (precisa de CMake, ECM, Qt 6 e KIO 6; sem isso, avisa e segue sem eles);
# --no-dolphin não instala os plugins.
# --from usa os programas já compilados em DIR (sem ele, compila com `cargo build --release`);
# --dry-run só lista o que instalaria; --no-activate não avisa o systemd nem o D-Bus nem atualiza
# os bancos de tipos e de aplicativos; --destdir grava em DIR/<prefixo> os arquivos que vão funcionar
# em <prefixo> (para montar os pacotes; implica --no-activate).
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
prefix="$HOME/.local" destdir="" from="" dolphin="" no_dolphin=false dry=false activate=true flavor=default
while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) prefix=$2; shift 2 ;;
    --destdir) destdir=$2; activate=false; shift 2 ;;
    --from) from=$2; shift 2 ;;
    --dolphin) dolphin=$2; shift 2 ;;
    --no-dolphin) no_dolphin=true; shift ;;
    --dry-run) dry=true; shift ;;
    --flavor) flavor=$2; shift 2 ;;
    --no-activate) activate=false; shift ;;
    *) echo "argumento desconhecido: $1" >&2; exit 2 ;;
  esac
done
table="$repo/packaging/gerasync/rename.sed"
# rename: o texto da entrada como fica na variante (no GeraSync, passado pela tabela).
case "$flavor" in
  default) rename() { cat; } ; package=gdrive-desktop-linux app_icons="$repo/assets/icons" ;;
  gerasync) rename() { sed -f "$table"; } ; package=gerasync app_icons="$repo/assets/gerasync/icons" ;;
  *) echo "variante desconhecida: $flavor" >&2; exit 2 ;;
esac
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
  cargo=cargo
  [ "$flavor" = default ] || cargo="sh $repo/scripts/cargo-gerasync.sh"
  (cd "$repo" && $cargo build --release -p gdl-daemon -p gdl-cli -p gdl-tray -p gdl-ui -p gdl-mcp --bins)
fi
target=target/release
[ "$flavor" = default ] || target=target/gerasync/release
from=${from:-$repo/$target}

# named <caminho>: o caminho instalado; o que vem depois do prefixo passa pela variante (o
# prefixo pode ter qualquer nome).
named() {
  printf '%s/%s\n' "$prefix" "$(printf '%s\n' "${1#"$prefix/"}" | rename)"
}

# O que foi instalado, um caminho por linha: a lista que a desinstalação apaga.
installed=""

# install_as <modo> <origem> <destino>: copia como está (programas e PNG), no caminho da variante.
install_as() {
  to=$(named "$3")
  echo "instala $destdir$to"
  installed="$installed$to
"
  $dry || install -Dm"$1" "$2" "$destdir$to"
}

# remove <arquivo>: tira um arquivo de uma instalação anterior, se houver.
remove() {
  set -- "$(named "$1")"
  [ -e "$destdir$1" ] || return 0
  echo "remove $destdir$1"
  $dry || rm -f "$destdir$1"
}

# put <origem> <destino>: copia um arquivo de texto com o conteúdo da variante, trocando
# /usr/bin/<programa> (no Exec) pelo programa instalado, entre aspas.
put() {
  to=$(named "$2")
  echo "instala $destdir$to"
  installed="$installed$to
"
  $dry && return
  mkdir -p "$(dirname "$destdir$to")"
  rename < "$1" | sed "s|/usr/bin/\([a-z-]*\)|\"$bin/\1\"|g" > "$destdir$to.tmp"
  chmod 644 "$destdir$to.tmp"
  mv "$destdir$to.tmp" "$destdir$to"
}

# Os programas: os do build têm os nomes de sempre; os do .tar.gz, os da variante.
for program in $programs; do
  source="$from/$(echo "$program" | rename)"
  [ -e "$source" ] || source="$from/$program"
  install_as 755 "$source" "$bin/$program"
done
put "$repo/packaging/systemd/gdrive-desktop-linuxd.service" "$share/systemd/user/gdrive-desktop-linuxd.service"
for file in "$repo"/packaging/dbus/*.service; do
  put "$file" "$share/dbus-1/services/$(basename "$file")"
done
for file in "$repo"/packaging/desktop/*.desktop; do
  put "$file" "$share/applications/$(basename "$file")"
done
put "$repo/packaging/mime/gdrive-desktop-linux.xml" "$share/mime/packages/gdrive-desktop-linux.xml"
# O ícone do app, nos tamanhos do tema (gerados por scripts/update-app-icon.py). O SVG de uma
# instalação anterior sai: o tema o preferiria aos PNG.
for file in "$app_icons"/hicolor/*/apps/*.png; do
  install_as 644 "$file" "$share/icons/hicolor/${file#"$app_icons/hicolor/"}"
done
remove "$share/icons/hicolor/scalable/apps/io.github.junior9283.GDriveDesktopLinux.svg"
# Os emblemas de estado do Dolphin, no tema do sistema (é nele que o Dolphin procura).
for file in "$repo"/assets/icons/hicolor/scalable/emblems/*.svg; do
  put "$file" "$share/icons/hicolor/scalable/emblems/$(basename "$file")"
done
# Os ícones da bandeja, no tema próprio que ela procura ao lado dos programas.
for file in "$repo"/assets/icons/hicolor/scalable/status/*.svg; do
  put "$file" "$share/gdrive-desktop-linux/icons/hicolor/scalable/status/$(basename "$file")"
done
# O menu do botão direito no Nautilus, no Nemo, no Caja e no Thunar: a mesma extensão na pasta de
# cada um (só carrega onde o pacote de Python do gerenciador estiver instalado).
for manager in nautilus nemo caja thunarx; do
  put "$repo/integrations/gtk/gdrive_desktop_linux_menu.py" \
    "$share/$manager-python/extensions/gdrive_desktop_linux_menu.py"
done

# Os plugins do Dolphin (menu do botão direito e emblemas de estado). Eles só vêm nos pacotes e
# nesta instalação: sem eles o Dolphin fica sem o submenu "Google Drive" e sem as nuvens.
if [ -z "$destdir" ] && ! $no_dolphin; then
  if [ -z "$dolphin" ] && ! $dry && [ "$prefix" = "$HOME/.local" ] &&
    [ -f "$repo/integrations/dolphin/build.sh" ]; then
    built=$(mktemp -d)
    trap 'rm -rf "$built"' EXIT
    if sh "$repo/integrations/dolphin/build.sh" --flavor "$flavor" "$built" >&2; then
      dolphin=$built
    else
      echo "aviso: não consegui compilar os plugins do Dolphin; o menu e os emblemas dele ficam de fora" >&2
    fi
  fi
  if [ -n "$dolphin" ]; then
    for plugin in kfileitemaction/gdrive_desktop_linux_menu overlayicon/gdrive_desktop_linux_overlay; do
      # O build.sh da variante já dá aos .so os nomes dela.
      install_as 755 "$dolphin/$(printf '%s\n' "${plugin##*/}" | rename).so" "$prefix/lib/qt6/plugins/kf6/$plugin.so"
    done
    conf="${XDG_CONFIG_HOME:-$HOME/.config}/environment.d/50-$package.conf"
    echo "instala $conf"
    installed="$installed$conf
"
    if ! $dry; then
      mkdir -p "$(dirname "$conf")"
      printf 'QT_PLUGIN_PATH=%s/lib/qt6/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}\n' "$prefix" > "$conf"
    fi
    echo "Os plugins do Dolphin só valem depois de sair e entrar de novo na sessão (o Qt lê o QT_PLUGIN_PATH ao iniciar)."
  fi
fi

# Os pacotes trazem um marcador do formato no lugar da lista (o gerenciador sabe o que é dele).
if ! $dry && [ -z "$destdir" ]; then
  printf '%s' "$installed" > "$share/$package/installed-files"
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
echo "Pronto. Abra \"Google Drive\" no menu de aplicativos (ou rode $bin/gdrive-desktop-linux-tray)." | rename
