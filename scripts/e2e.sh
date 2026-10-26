#!/bin/sh
# Testes de ponta a ponta da janela: o serviço de teste (Drive simulado, conta conectada), a
# janela num servidor gráfico próprio (Xvfb) e num barramento de sessão próprio, comandada pelo
# tauri-driver, com o openbox cuidando do foco das janelas (sem gerenciador de janelas, o Xvfb
# tira o foco da janela em momentos aleatórios). Nada aparece na tela de quem roda. Sai com 77
# (pulado) sem o WebKitWebDriver, o tauri-driver, o openbox ou o xdotool.
# Um argumento filtra os testes pelo nome (`scripts/e2e.sh search`).
set -eu
cd "$(dirname "$0")/.."

if [ "${1:-}" != "--inside" ]; then
  if ! command -v WebKitWebDriver >/dev/null 2>&1; then
    echo "e2e pulado: falta o WebKitWebDriver (pacote webkitgtk6.0)" >&2
    exit 77
  fi
  if ! command -v tauri-driver >/dev/null 2>&1; then
    echo "e2e pulado: falta o tauri-driver (cargo install tauri-driver --locked)" >&2
    exit 77
  fi
  if ! command -v openbox >/dev/null 2>&1; then
    echo "e2e pulado: falta o openbox (pacote openbox)" >&2
    exit 77
  fi
  if ! command -v xdotool >/dev/null 2>&1; then
    echo "e2e pulado: falta o xdotool (pacote xdotool)" >&2
    exit 77
  fi
  cargo build -q -p gdl-ui -p gdl-tray --bins
  cargo build -q -p gdl-daemon --example e2e-daemon
  cargo test -q -p gdl-ui --test e2e --no-run
  exec env -u DISPLAY -u WAYLAND_DISPLAY \
    dbus-run-session --config-file=scripts/e2e-bus.conf -- xvfb-run -a -s "-screen 0 1920x1080x24" "$0" --inside "$@"
fi
shift

# Daqui para baixo: dentro do barramento e do Xvfb.
work=$(mktemp -d)
export XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache"
export NO_AT_BRIDGE=1
# Sem WAYLAND_DISPLAY, o GTK ainda tenta o wayland-0 e abriria as janelas na sessão de quem roda os
# testes: só o X do Xvfb.
export GDK_BACKEND=x11
# Os testes conferem os textos em pt-BR.
export LANG=pt_BR.UTF-8 LC_ALL=pt_BR.UTF-8
pids=""
cleanup() {
  for pid in $pids; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

openbox --sm-disable > "$work/wm.log" 2>&1 &
pids="$pids $!"
# O que o "navegador" do serviço de teste abriu, o estado do armazenamento simulado e onde os
# testes acham a bandeja.
export GDL_E2E_OPENED="$work/opened" GDL_E2E_QUOTA="$work/quota" GDL_E2E_BIN="$PWD/target/debug"
# A janela abre os links pelo navegador das preferências dela: um que só anota, como o do serviço.
mkdir -p "$XDG_CONFIG_HOME/gdrive-desktop-linux"
printf '#!/bin/sh\nprintf "%%s\\n" "$1" >> "%s"\n' "$GDL_E2E_OPENED" > "$work/navegador"
chmod +x "$work/navegador"
printf 'DefaultWebBrowser = "%s"\n' "$work/navegador" > "$XDG_CONFIG_HOME/gdrive-desktop-linux/preferences.toml"
target/debug/examples/e2e-daemon > "$work/daemon.out" 2> "$work/daemon.log" &
pids="$pids $!"
export GDL_E2E_DAEMON_PID=$!
tauri-driver --port 4444 > "$work/driver.log" 2>&1 &
pids="$pids $!"

i=0
until grep -q pronto "$work/daemon.out" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 300 ]; then
    echo "o serviço de teste não subiu:" >&2
    tail -20 "$work/daemon.log" >&2
    exit 1
  fi
  sleep 0.1
done

GDL_E2E_WEBDRIVER=http://127.0.0.1:4444 cargo test -q -p gdl-ui --test e2e -- --ignored --test-threads=1 "$@"
