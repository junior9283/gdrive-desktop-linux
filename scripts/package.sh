#!/bin/sh
# Monta os instaladores de uma versão em <saída>: .deb (Debian, Ubuntu, Mint), .rpm (Fedora,
# openSUSE), .pkg.tar.zst (Arch, Manjaro) e um .tar.gz para qualquer distribuição, com o
# install.sh que instala em ~/.local sem root.
#
#   scripts/package.sh [--from DIR] <saída>
#
# --from usa os programas já compilados em DIR (sem ele, compila com `cargo build --release`).
# Os pacotes saem do nfpm (https://nfpm.goreleaser.com); NFPM troca o comando (ex.: um contêiner).
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
from=""
while [ $# -gt 1 ]; do
  case "$1" in
    --from) from=$2; shift 2 ;;
    *) echo "argumento desconhecido: $1" >&2; exit 2 ;;
  esac
done
[ $# -eq 1 ] || { echo "uso: scripts/package.sh [--from DIR] <saída>" >&2; exit 2; }
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
} > "$work/nfpm.yaml"
for packager in deb rpm archlinux; do
  VERSION=$version ${NFPM:-nfpm} package --config "$work/nfpm.yaml" --packager "$packager" \
    --target "$out/"
done

# O .tar.gz: os programas (os mesmos dos pacotes) e o que o install-dev.sh precisa ao lado deles.
dir="$work/$name-$version"
mkdir -p "$dir/scripts" "$dir/packaging" "$dir/assets/icons"
cp -r "$work/stage/usr/bin" "$dir/"
cp "$repo/scripts/install-dev.sh" "$dir/scripts/"
cp -r "$repo/packaging/systemd" "$repo/packaging/dbus" "$repo/packaging/desktop" "$repo/packaging/mime" \
  "$dir/packaging/"
cp -r "$repo/assets/icons/hicolor" "$dir/assets/icons/"
cp "$repo/LICENSE" "$repo/README.md" "$repo/packaging/install.sh" "$dir/"
tar -C "$work" -czf "$out/$name-$version-x86_64.tar.gz" "$name-$version"

(cd "$out" && sha256sum -- *.deb *.rpm *.pkg.tar.zst *.tar.gz > SHA256SUMS)
ls -1 "$out"
