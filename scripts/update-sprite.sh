#!/bin/sh
# Refaz crates/gdl-ui/ui/icons/sprite.svg com só os Material Symbols (Apache-2.0) que as telas
# usam. Para um ícone novo, acrescente o nome em SYMBOLS e rode de novo.
set -eu
SYMBOLS="cloud_off"
BASE=https://raw.githubusercontent.com/google/material-design-icons/master/symbols/web
OUT="$(dirname "$0")/../crates/gdl-ui/ui/icons/sprite.svg"
{
  printf '<svg xmlns="http://www.w3.org/2000/svg">\n'
  for name in $SYMBOLS; do
    svg=$(curl -sSfL "$BASE/$name/materialsymbolsoutlined/${name}_24px.svg")
    viewbox=$(printf '%s' "$svg" | sed -n 's/.*viewBox="\([^"]*\)".*/\1/p')
    paths=$(printf '%s' "$svg" | sed 's/^<svg[^>]*>//; s/<\/svg>$//')
    printf '  <symbol id="%s" viewBox="%s">%s</symbol>\n' "$name" "$viewbox" "$paths"
  done
  printf '</svg>\n'
} > "$OUT.tmp"
mv "$OUT.tmp" "$OUT"
