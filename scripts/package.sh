#!/bin/sh
# Monta os instaladores de uma versão em <saída>: .deb (Debian, Ubuntu, Mint), .rpm (Fedora,
# openSUSE), .pkg.tar.zst (Arch, Manjaro) e um .tar.gz para qualquer distribuição, com o
# install.sh que instala em ~/.local sem root.
#
#   scripts/package.sh [--from DIR] [--dolphin PASTA] <saída>
#
# --from usa os programas já compilados em DIR (sem ele, compila com `cargo build --release`).
# --dolphin põe nos pacotes os plugins do Dolphin (menu e emblemas) da PASTA que o
# integrations/dolphin/build.sh preenche; eles não vão no .tar.gz, que instala em ~/.local, fora
# do caminho de plugins do Qt.
# Os pacotes saem do nfpm (https://nfpm.goreleaser.com); NFPM troca o comando (ex.: um contêiner).
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
from="" dolphin=""
while [ $# -gt 1 ]; do
  case "$1" in
    --from) from=$2; shift 2 ;;
    --dolphin) dolphin=$(cd "$2" && pwd); shift 2 ;;
    *) echo "argumento desconhecido: $1" >&2; exit 2 ;;
  esac
done
[ $# -eq 1 ] || { echo "uso: scripts/package.sh [--from DIR] [--dolphin PASTA] <saída>" >&2; exit 2; }
mkdir -p "$1"
out=$(cd "$1" && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -n 1)
name=gdrive-desktop-linux

if [ -z "$from" ]; then
  (cd "$repo" && cargo build --release -p gdl-daemon -p gdl-cli -p gdl-tray -p gdl-ui --bins)
  from=$repo/target/release
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Os pacotes do sistema: os mesmos arquivos da instalação do usuário, em /usr.
sh "$repo/scripts/install-dev.sh" --prefix /usr --destdir "$work/stage" --from "$from" >/dev/null
# Só os arquivos, um a um: as pastas (/usr, /usr/bin…) continuam sendo do sistema.
{
  cat "$repo/packaging/nfpm.yaml"
  echo "contents:"
  (cd "$work/stage" && find usr -type f | sort) | while read -r file; do
    mode=0644
    case "$file" in usr/bin/*) mode=0755 ;; esac
    printf '  - src: "%s"\n    dst: "/%s"\n    file_info:\n      mode: %s\n' \
      "$work/stage/$file" "$file" "$mode"
  done
  # Cada pacote diz o próprio formato: é por ele que a desinstalação escolhe o gerenciador.
  for packager in deb rpm archlinux; do
    echo "$packager" > "$work/package-$packager"
    printf '  - src: "%s"\n    dst: "/usr/share/%s/package"\n    packager: %s\n    file_info:\n      mode: 0644\n' \
      "$work/package-$packager" "$name" "$packager"
  done
  # Os plugins do Dolphin, na pasta de plugins do Qt de cada formato. Sem dependência do KDE: o
  # KIO só os carrega quando já está rodando (no Dolphin), e fora dele os arquivos ficam parados.
  [ -z "$dolphin" ] || for target in deb:/usr/lib/x86_64-linux-gnu rpm:/usr/lib64 archlinux:/usr/lib; do
    for plugin in kfileitemaction/gdrive_desktop_linux_menu overlayicon/gdrive_desktop_linux_overlay; do
      printf '  - src: "%s"\n    dst: "%s/qt6/plugins/kf6/%s.so"\n    packager: %s\n    file_info:\n      mode: 0644\n' \
        "$dolphin/${plugin#*/}.so" "${target#*:}" "$plugin" "${target%%:*}"
    done
  done
} > "$work/nfpm.yaml"
for packager in deb rpm archlinux; do
  VERSION=$version ${NFPM:-nfpm} package --config "$work/nfpm.yaml" --packager "$packager" \
    --target "$out/"
done

# O .tar.gz: os programas (os mesmos dos pacotes) e o que o install-dev.sh precisa ao lado deles.
dir="$work/$name-$version"
mkdir -p "$dir/scripts" "$dir/packaging" "$dir/assets/icons" "$dir/integrations/gtk"
cp -r "$work/stage/usr/bin" "$dir/"
cp "$repo/scripts/install-dev.sh" "$dir/scripts/"
cp -r "$repo/packaging/systemd" "$repo/packaging/dbus" "$repo/packaging/desktop" "$repo/packaging/mime" \
  "$dir/packaging/"
cp -r "$repo/assets/icons/hicolor" "$dir/assets/icons/"
cp "$repo/integrations/gtk/gdrive_desktop_linux_menu.py" "$dir/integrations/gtk/"
cp "$repo/LICENSE" "$repo/README.md" "$repo/packaging/install.sh" "$repo/packaging/uninstall.sh" "$dir/"
tar -C "$work" -czf "$out/$name-$version-x86_64.tar.gz" "$name-$version"

(cd "$out" && sha256sum -- *.deb *.rpm *.pkg.tar.zst *.tar.gz > SHA256SUMS)
ls -1 "$out"
