# O que muda de uma variante para a outra, lido com `. "$repo/scripts/flavor.sh"` (com $repo e
# $flavor já definidos) pelo install-dev.sh e pelo package.sh:
#   rename     o texto da entrada como fica na variante (no GeraSync, passado pela tabela);
#   name       o nome do pacote, dos programas e das pastas;
#   app_icons  a pasta dos ícones do app;
#   cargo, target  o comando que compila e onde os programas saem, relativos ao $repo (o caminho
#              dele pode ter espaços: o build roda dentro de cd "$repo").
case "$flavor" in
  default)
    rename() { cat; }
    name=gdrive-desktop-linux app_icons="$repo/assets/icons" cargo=cargo target=target/release ;;
  gerasync)
    rename() { sed -f "$repo/packaging/gerasync/rename.sed"; }
    name=gerasync app_icons="$repo/assets/gerasync/icons" cargo="sh scripts/cargo-gerasync.sh"
    target=target/gerasync/release ;;
  *) echo "variante desconhecida: $flavor" >&2; exit 2 ;;
esac
