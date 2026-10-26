#!/bin/sh
# O install.sh do .tar.gz do release: instala os programas que vêm ao lado dele para o seu usuário,
# sem root, em ~/.local (ou em --prefix DIR). Aceita as opções do scripts/install-dev.sh.
here=$(cd "$(dirname "$0")" && pwd)
exec sh "$here/scripts/install-dev.sh" --from "$here/bin" "$@"
