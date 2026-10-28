"""O submenu "Google Drive" do botão direito no Nautilus, no Nemo, no Caja e no Thunar.

O mesmo arquivo vai para a pasta de extensões de cada um (nautilus-python, nemo-python,
caja-python e thunarx-python) e sabe em qual está pela pasta de onde foi carregado. Quais itens aparecem, com quais
nomes e marcas, e o que cada um faz é decidido pelo serviço (métodos `Menu` e `MenuAction`);
aqui só se desenha a resposta. Fora do Meu Drive, ou com o serviço parado, não aparece nada.
"""

import os

from gi.repository import Gio, GLib, GObject

SERVICE = "io.github.junior9283.GDriveDesktopLinux1"
PATH = "/io/github/junior9283/GDriveDesktopLinux1"
INTERFACE = SERVICE + ".Service"
APP_ICON = "io.github.junior9283.GDriveDesktopLinux"
TITLE = "Google Drive"
# O menu abre na hora: o serviço responde do banco local, e passando disso ele fica de fora.
MENU_TIMEOUT_MS = 500
# A pasta de extensões de cada gerenciador e o módulo dele.
HOSTS = {"nautilus-python": "Nautilus", "nemo-python": "Nemo", "caja-python": "Caja",
         "thunarx-python": "Thunarx"}


def host_module_name(path):
    """O módulo do gerenciador que carrega extensões de `path`
    (`…/nautilus-python/extensions/este.py`), ou `None` fora dessas pastas."""
    parts = os.path.normpath(path).split(os.sep)
    return next((HOSTS[part] for part in reversed(parts) if part in HOSTS), None)


def menu_entries(paths, bus=None):
    """`(id, rótulo, marca)` de cada item para `paths`; vazio fora do Drive ou sem o serviço
    (que não é iniciado só para isso)."""
    if not paths:
        return []
    try:
        bus = bus or Gio.bus_get_sync(Gio.BusType.SESSION)
        reply = bus.call_sync(
            SERVICE, PATH, INTERFACE, "Menu", GLib.Variant("(as)", (paths,)),
            GLib.VariantType("(a(sss))"), Gio.DBusCallFlags.NO_AUTO_START, MENU_TIMEOUT_MS, None)
    except GLib.Error:
        return []
    return reply.unpack()[0]


def run_action(action, paths, on_text, on_error, bus=None):
    """Faz a ação sem esperar (criar um documento vai ao Drive): `on_text` recebe o texto a
    copiar (o link), se houver, e `on_error` a mensagem do serviço."""
    bus = bus or Gio.bus_get_sync(Gio.BusType.SESSION)

    def done(conn, result):
        try:
            text = conn.call_finish(result).unpack()[0]
        except GLib.Error as e:
            # Sem o "GDBus.Error:<nome do erro>: " da frente, só o texto do serviço.
            remote = Gio.DBusError.is_remote_error(e)
            on_error(e.message.split(": ", 1)[-1] if remote else e.message)
            return
        if text:
            on_text(text)

    bus.call(SERVICE, PATH, INTERFACE, "MenuAction", GLib.Variant("(sas)", (action, paths)),
             GLib.VariantType("(s)"), Gio.DBusCallFlags.NONE, -1, None, done)


# Sem caixa de seleção nos menus do GTK, a marca vai no nome: "✓" ligado, "–" em parte.
MARKS = {"on": "✓ ", "mixed": "– "}


def build(module, entries, activate):
    """O item "Google Drive" com o submenu, nas classes de `module`; `activate(id)` roda ao
    escolher um item. Quem não tem separador (só o Nemo tem) fica sem; quem não tem marca de
    seleção ganha a marca (`MARKS`) no nome."""
    menu = module.Menu()
    separator = getattr(module.MenuItem, "new_separator", None)
    for index, (id, label, state) in enumerate(entries):
        name = f"GDriveDesktopLinux::{index}"
        if id == "-":
            if separator:
                menu.append_item(separator(name))
            continue
        item = module.MenuItem(name=name, label=MARKS.get(state, "") + label)
        item.connect("activate", lambda _item, *_, id=id: activate(id))
        menu.append_item(item)
    top = module.MenuItem(name="GDriveDesktopLinux::menu", label=TITLE, icon=APP_ICON)
    # O Thunar chama de `set_menu` o que os outros chamam de `set_submenu`.
    (getattr(top, "set_submenu", None) or top.set_menu)(menu)
    return top


def copy_to_clipboard(text):
    from gi.repository import Gdk

    display = Gdk.Display.get_default()
    if hasattr(display, "get_clipboard"):  # GTK 4 (Nautilus 43 em diante)
        display.get_clipboard().set_content(Gdk.ContentProvider.new_for_value(text))
        return
    from gi.repository import Gtk

    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    clipboard.set_text(text, -1)
    clipboard.store()


def notify_error(message):
    """O erro vira uma notificação da área de trabalho (o gerenciador não tem onde mostrar)."""
    bus = Gio.bus_get_sync(Gio.BusType.SESSION)
    bus.call("org.freedesktop.Notifications", "/org/freedesktop/Notifications",
             "org.freedesktop.Notifications", "Notify",
             GLib.Variant("(susssasa{sv}i)", (TITLE, 0, APP_ICON, TITLE, message, [], {}, -1)),
             None, Gio.DBusCallFlags.NONE, -1, None, None)


def items_for(module, files):
    """O que o gerenciador mostra para `files`: o item "Google Drive" ou nada."""
    paths = [f.get_location().get_path() for f in files]
    if None in paths:  # fora do disco (rede, lixeira…)
        return []
    entries = menu_entries(paths)
    if not entries:
        return []
    return [build(module, entries, lambda id: run_action(id, paths, copy_to_clipboard, notify_error))]


def provider(module):
    """A classe que o gerenciador carrega, nas interfaces de `module`."""

    class GDriveDesktopLinuxMenu(GObject.GObject, module.MenuProvider):
        # Nautilus 43+: (files); Nautilus antigo, Nemo e Caja: (window, files).
        def get_file_items(self, *args):
            return items_for(module, args[-1])

        # Thunar.
        def get_file_menu_items(self, window, files):
            return items_for(module, files)

    return GDriveDesktopLinuxMenu


_host = host_module_name(__file__)
if _host is not None:
    import gi.repository

    GDriveDesktopLinuxMenu = provider(getattr(gi.repository, _host))
