"""Testes da extensão do Nautilus, do Nemo, do Caja e do Thunar, num barramento D-Bus próprio
(`run.sh`) e com módulos falsos no lugar dos de cada gerenciador, que não precisam estar
instalados. O serviço falso é o de `integrations/testing/fake_service.py`."""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from gi.repository import GLib, GObject

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import gdrive_desktop_linux_menu as ext  # noqa: E402


class Item:
    def __init__(self, name, label, icon=None):
        self.name, self.label, self.icon = name, label, icon
        self.handlers, self.submenu = [], None

    def connect(self, signal, handler):
        assert signal == "activate"
        self.handlers.append(handler)

    def activate(self):
        for handler in self.handlers:
            handler(self)


class Menu:
    def __init__(self):
        self.items = []

    def append_item(self, item):
        self.items.append(item)


class Nautilus:
    """Como o nautilus-python e o caja-python: sem separadores, `set_submenu`."""

    class MenuProvider:
        pass

    class MenuItem(Item):
        def set_submenu(self, menu):
            self.submenu = menu

    Menu = Menu


class Nemo(Nautilus):
    class MenuItem(Nautilus.MenuItem):
        @staticmethod
        def new_separator(name):
            return Item(name, "---")


class Thunarx:
    """Como o thunarx-python: `set_menu`."""

    class MenuProvider:
        pass

    class MenuItem(Item):
        def set_menu(self, menu):
            self.submenu = menu

    Menu = Menu


class File:
    def __init__(self, path):
        self.path = path

    def get_location(self):
        return self

    def get_path(self):
        return self.path


def labels(top):
    return [item.label for item in top.submenu.items]


def wait_for(predicate):
    context = GLib.MainContext.default()
    for _ in range(200):
        if predicate():
            return
        context.iteration(False)
        GLib.usleep(10_000)
    raise AssertionError("não chegou a tempo")


class WithoutService(unittest.TestCase):
    def test_no_service_no_menu(self):
        self.assertEqual(ext.menu_entries(["/drive/a.txt"]), [])
        self.assertEqual(ext.items_for(Nautilus, [File("/drive/a.txt")]), [])


class WithService(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.drive = os.path.join(cls.tmp.name, "drive")
        cls.log = os.path.join(cls.tmp.name, "actions")
        fake = HERE.parent.parent / "testing" / "fake_service.py"
        cls.fake = subprocess.Popen(
            [sys.executable, "-I", "-W", "ignore::DeprecationWarning", str(fake), cls.log, cls.drive],
            stdout=subprocess.PIPE, text=True)
        assert cls.fake.stdout.readline().strip() == "pronto"

    @classmethod
    def tearDownClass(cls):
        cls.fake.kill()
        cls.fake.wait()
        cls.tmp.cleanup()

    def path(self, name):
        return os.path.join(self.drive, name)

    def test_one_file_shows_everything_with_marks(self):
        [top] = ext.items_for(Nautilus, [File(self.path("a.txt"))])
        self.assertEqual((top.label, top.icon), ("Google Drive", ext.APP_ICON))
        self.assertEqual(labels(top), [
            "Abrir no navegador", "Copiar link", "✓ Disponível off-line", "Somente on-line",
            "Ver itens na lixeira"])

    def test_partly_offline_is_marked_with_a_dash(self):
        entries = [("offline", "Disponível off-line (em parte)", "mixed"), ("online-only", "Somente on-line", "off")]
        top = ext.build(Nautilus, entries, lambda _id: None)
        self.assertEqual(labels(top), ["– Disponível off-line (em parte)", "Somente on-line"])

    def test_nemo_keeps_the_separators_and_thunar_uses_set_menu(self):
        [nemo] = ext.items_for(Nemo, [File(self.path("a.txt"))])
        self.assertEqual(labels(nemo)[2], "---")
        [thunar] = ext.items_for(Thunarx, [File(self.path("a.txt")), File(self.path("b"))])
        self.assertEqual(labels(thunar), [
            "✓ Disponível off-line", "Somente on-line", "Ver itens na lixeira"])

    def test_outside_the_drive_or_off_disk_nothing_shows(self):
        self.assertEqual(ext.items_for(Nautilus, [File(self.path("a.txt")), File("/home/x")]), [])
        self.assertEqual(ext.items_for(Nautilus, [File(None)]), [])
        self.assertEqual(ext.items_for(Nautilus, []), [])

    def test_actions_reach_the_service_and_report_back(self):
        texts, errors = [], []
        a = self.path("a.txt")
        ext.run_action("copy-link", [a], texts.append, errors.append)
        wait_for(lambda: texts)
        self.assertEqual(texts, ["https://drive.google.com/file/d/x/view"])
        ext.run_action("online-only", [a], texts.append, errors.append)
        wait_for(lambda: errors)
        self.assertEqual(errors, ["sem internet"])
        with open(self.log) as log:
            self.assertIn("copy-link a.txt\nonline-only a.txt\n", log.read())

    def test_choosing_an_item_runs_its_action(self):
        chosen = []
        entries = [("open-web", "Abrir", ""), ("-", "", ""), ("trash", "Lixeira", "")]
        top = ext.build(Nautilus, entries, chosen.append)
        for item in top.submenu.items:
            item.activate()
        self.assertEqual(chosen, ["open-web", "trash"])


class Host(unittest.TestCase):
    def test_each_file_manager_finds_its_module_by_folder(self):
        for path, module in [
            ("/usr/share/nautilus-python/extensions/m.py", "Nautilus"),
            ("/home/ana/.local/share/nemo-python/extensions/m.py", "Nemo"),
            ("/usr/share/caja-python/extensions/m.py", "Caja"),
            ("/usr/share/thunarx-python/extensions/m.py", "Thunarx"),
            (str(HERE.parent / "gdrive_desktop_linux_menu.py"), None),
        ]:
            self.assertEqual(ext.host_module_name(path), module, path)

    def test_the_provider_answers_both_signatures(self):
        provider = ext.provider(Nautilus)()
        self.assertIsInstance(provider, GObject.GObject)
        self.assertEqual(provider.get_file_items([File(None)]), [])
        self.assertEqual(provider.get_file_items(object(), [File(None)]), [])
        self.assertEqual(ext.provider(Thunarx)().get_file_menu_items(object(), [File(None)]), [])


if __name__ == "__main__":
    unittest.main()
