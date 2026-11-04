#!/bin/sh
# O release do GeraSync só sai com o Client ID oficial: confere, antes de compilar, que
# GERASYNC_CLIENT_ID e GERASYNC_CLIENT_SECRET (os segredos do repositório no GitHub) vieram, não são
# os de mentira do CI e que GERASYNC_ALLOW_FAKE_CREDS não está ligado. Nunca mostra os valores.
# O build.rs do gdl-daemon já recusa os segredos ausentes; esta checagem falha em segundos, antes
# de compilar, e pega os de mentira (FAKE_ID e FAKE_SECRET do build.rs, os mesmos valores).
#
#   scripts/check-gerasync-secrets.sh
set -eu
if [ -n "${GERASYNC_ALLOW_FAKE_CREDS:-}" ]; then
  echo "GERASYNC_ALLOW_FAKE_CREDS não pode estar no release" >&2
  exit 1
fi
for var in GERASYNC_CLIENT_ID GERASYNC_CLIENT_SECRET; do
  eval "value=\${$var:-}"
  if [ -z "$value" ]; then
    echo "falta o segredo $var" >&2
    exit 1
  fi
  case "$value" in
    gerasync-fake.apps.googleusercontent.com | fake-secret) echo "o segredo $var tem um valor de mentira" >&2; exit 1 ;;
  esac
done
