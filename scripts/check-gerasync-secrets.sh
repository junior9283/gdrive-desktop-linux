#!/bin/sh
# O release do GeraSync só sai com o Client ID oficial: confere, antes de compilar, que
# GERASYNC_CLIENT_ID e GERASYNC_CLIENT_SECRET (os segredos do repositório no GitHub) vieram, não são
# os de mentira do CI, não estão trocados (o ID tem o formato de Client ID) e que
# GERASYNC_ALLOW_FAKE_CREDS não está ligado. Nunca mostra os valores.
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
# Na 1.3.0 os dois foram cadastrados trocados e o login dava "invalid_client".
# O "x" na frente impede o expr de ler o valor como operador.
if ! expr "x$GERASYNC_CLIENT_ID" : 'x[0-9]\{1,\}-[a-z0-9]\{1,\}\.apps\.googleusercontent\.com$' >/dev/null; then
  echo "GERASYNC_CLIENT_ID não tem o formato <número>-<código>.apps.googleusercontent.com (trocado com o segredo?)" >&2
  exit 1
fi
case "$GERASYNC_CLIENT_SECRET" in
  *.apps.googleusercontent.com) echo "GERASYNC_CLIENT_SECRET parece um Client ID (trocado com o ID?)" >&2; exit 1 ;;
esac
