#!/bin/sh
# Roda o cargo para a variante GeraSync: o --cfg gerasync (acrescentado ao RUSTFLAGS de quem
# chama, que no CI já traz -D warnings), um target/ próprio (sem recompilar a variante de sempre a
# cada troca) e a identidade da janela no Tauri.
#
#   scripts/cargo-gerasync.sh <argumentos do cargo>
#
# As credenciais vêm de GERASYNC_CLIENT_ID e GERASYNC_CLIENT_SECRET; para compilar e testar sem
# elas, GERASYNC_ALLOW_FAKE_CREDS=1 (crates/gdl-daemon/build.rs).
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
export RUSTFLAGS="${RUSTFLAGS:-} --cfg gerasync"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target/gerasync}"
export TAURI_CONFIG='{"identifier":"io.github.junior9283.GeraSync","productName":"gerasync"}'
exec cargo "$@"
