#!/usr/bin/env python3
"""Serviço falso para os testes das extensões dos gerenciadores de arquivos: responde `Menu` e
`MenuAction` como o serviço real, com o "Meu Drive" na pasta do segundo argumento. Cada
`MenuAction` vai para o arquivo do primeiro argumento."""

import sys

from gi.repository import Gio, GLib

NAME = "io.github.junior9283.GDriveDesktopLinux1"
PATH = "/io/github/junior9283/GDriveDesktopLinux1"
XML = f"""
<node><interface name="{NAME}.Service">
  <method name="Menu"><arg type="as" direction="in"/><arg type="a(sss)" direction="out"/></method>
  <method name="MenuAction">
    <arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="s" direction="out"/>
  </method>
</interface></node>
"""
log = open(sys.argv[1], "a", buffering=1)
drive = sys.argv[2].rstrip("/") + "/"


def menu(paths):
    if not paths or not all(p.startswith(drive) for p in paths):
        return []
    entries = [("offline", "Disponível off-line", "on"), ("online-only", "Somente on-line", "off")]
    if len(paths) == 1:
        entries = [("open-web", "Abrir no navegador", ""), ("copy-link", "Copiar link", ""),
                   ("-", "", "")] + entries
    return entries + [("-", "", ""), ("trash", "Ver itens na lixeira", "")]


def call(conn, sender, path, iface, method, params, invocation):
    if method == "Menu":
        invocation.return_value(GLib.Variant("(a(sss))", (menu(params[0]),)))
        return
    action, paths = params
    log.write(" ".join([action] + [p.removeprefix(drive) for p in paths]) + "\n")
    if action == "copy-link":
        invocation.return_value(GLib.Variant("(s)", ("https://drive.google.com/file/d/x/view",)))
    elif action == "trash":
        invocation.return_value(GLib.Variant("(s)", ("",)))
    else:
        invocation.return_dbus_error(f"{NAME}.Error.Failed", "sem internet")


def acquired(conn, name):
    node = Gio.DBusNodeInfo.new_for_xml(XML)
    conn.register_object(PATH, node.interfaces[0], call, None, None)


Gio.bus_own_name(Gio.BusType.SESSION, NAME, 0, acquired, lambda *a: print("pronto", flush=True), None)
GLib.MainLoop().run()
