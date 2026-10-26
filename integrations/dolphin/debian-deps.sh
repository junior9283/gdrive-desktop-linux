#!/bin/sh
# Instala o que o integrations/dolphin/build.sh precisa num Debian 13 (o contêiner do CI e do
# release): o CMake, o ECM, o Qt 6 e o KIO 6 para compilar; o PyGObject e o D-Bus para os testes.
set -eu
apt-get update
apt-get install -y cmake g++ extra-cmake-modules qt6-base-dev libkf6kio-dev libkf6coreaddons-dev \
  python3-gi dbus
