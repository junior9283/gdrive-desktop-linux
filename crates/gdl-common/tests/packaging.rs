//! A instalação para uso (`scripts/install-dev.sh`) e os arquivos de `packaging/`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROGRAMS: [&str; 4] = [
    "gdrive-desktop-linuxd",
    "gdrive-desktop-linux",
    "gdrive-desktop-linux-tray",
    "gdrive-desktop-linux-ui",
];

/// O que a instalação põe em `<prefixo>` (fora os programas).
const FILES: &[&str] = &[
    "share/systemd/user/gdrive-desktop-linuxd.service",
    "share/dbus-1/services/io.github.junior9283.GDriveDesktopLinux1.service",
    "share/dbus-1/services/io.github.junior9283.GDriveDesktopLinux1.Ui.service",
    "share/applications/io.github.junior9283.GDriveDesktopLinux.desktop",
    "share/applications/io.github.junior9283.GDriveDesktopLinux.OpenPointer.desktop",
    "share/applications/io.github.junior9283.GDriveDesktopLinux.Uninstall.desktop",
    "share/mime/packages/gdrive-desktop-linux.xml",
    "share/icons/hicolor/48x48/apps/io.github.junior9283.GDriveDesktopLinux.png",
    "share/icons/hicolor/256x256/apps/io.github.junior9283.GDriveDesktopLinux.png",
    "share/gdrive-desktop-linux/icons/hicolor/scalable/status/gdl-synced.svg",
    "share/gdrive-desktop-linux/icons/hicolor/scalable/status/gdl-syncing-4.svg",
    "share/nautilus-python/extensions/gdrive_desktop_linux_menu.py",
    "share/nemo-python/extensions/gdrive_desktop_linux_menu.py",
    "share/caja-python/extensions/gdrive_desktop_linux_menu.py",
    "share/thunarx-python/extensions/gdrive_desktop_linux_menu.py",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str]) -> Output {
    run_with(args, None)
}

/// Roda o `install-dev.sh` com `args` e, se dado, outro `PATH`.
fn run_with(args: &[&str], path: Option<&str>) -> Output {
    let mut command = Command::new("sh");
    command
        .arg(repo().join("scripts/install-dev.sh"))
        .args(args);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().unwrap()
}

/// Roda o `install-dev.sh` com `args` e devolve o que ele escreveu.
fn install(args: &[&str]) -> String {
    succeeded(run(args))
}

/// Confere que a instalação deu certo e devolve o que ela escreveu.
fn succeeded(out: Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Programas de mentira em `<dir>/programas`, para o `--from`.
fn fake_bins(dir: &Path) -> PathBuf {
    let bins = dir.join("programas");
    std::fs::create_dir_all(&bins).unwrap();
    for program in PROGRAMS {
        std::fs::write(bins.join(program), "#!/bin/sh\n").unwrap();
    }
    bins
}

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn dry_run_lists_every_file_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let prefix = dir.path().join("com espaço");

    let listed = install(&["--prefix", prefix.to_str().unwrap(), "--dry-run"]);

    let wanted = PROGRAMS
        .iter()
        .map(|p| format!("bin/{p}"))
        .chain(FILES.iter().map(|f| f.to_string()));
    for file in wanted {
        let path = prefix.join(&file).display().to_string();
        assert!(
            listed.lines().any(|l| l.ends_with(&path)),
            "{file}:\n{listed}"
        );
    }
    assert!(!prefix.exists());
}

#[test]
fn installs_with_the_paths_of_the_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let (bins, prefix) = (fake_bins(dir.path()), dir.path().join("com espaço"));

    install(&[
        "--prefix",
        prefix.to_str().unwrap(),
        "--from",
        bins.to_str().unwrap(),
        "--no-activate",
    ]);

    for program in PROGRAMS {
        let mode = std::fs::metadata(prefix.join("bin").join(program))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "{program}");
    }
    for file in FILES {
        assert!(prefix.join(file).is_file(), "{file}");
    }
    // O caminho vai entre aspas: o systemd, o D-Bus e o .desktop separam o Exec nos espaços.
    let bin = prefix.join("bin").display().to_string();
    let wanted = [
        (
            FILES[0],
            format!("ExecStart=\"{bin}/gdrive-desktop-linuxd\"\n"),
        ),
        (FILES[1], format!("Exec=\"{bin}/gdrive-desktop-linuxd\"\n")),
        (
            FILES[2],
            format!("Exec=\"{bin}/gdrive-desktop-linux-ui\" --dbus\n"),
        ),
        // Fora do PATH (um prefixo qualquer), o menu e os ponteiros ainda acham os programas.
        (
            FILES[3],
            format!("Exec=\"{bin}/gdrive-desktop-linux-tray\"\n"),
        ),
        (
            FILES[4],
            format!("Exec=\"{bin}/gdrive-desktop-linux\" open-pointer %f\n"),
        ),
        (
            FILES[5],
            format!("Exec=\"{bin}/gdrive-desktop-linux-ui\" --open uninstall\n"),
        ),
    ];
    // A lista que a desinstalação apaga: tudo o que foi instalado, e só isso.
    let mut listed: Vec<String> = read(prefix.join("share/gdrive-desktop-linux/installed-files"))
        .lines()
        .map(String::from)
        .collect();
    listed.sort();
    let every = PROGRAMS
        .iter()
        .map(|p| format!("bin/{p}"))
        .chain(FILES.iter().map(|f| f.to_string()));
    for file in every {
        let path = prefix.join(&file).display().to_string();
        assert!(listed.contains(&path), "{file} fora da lista");
    }
    for path in &listed {
        assert!(Path::new(path).is_file(), "{path} na lista sem existir");
    }
    for (file, line) in wanted {
        let body = read(prefix.join(file));
        assert!(body.contains(&line), "{file}: {line}\n{body}");
    }
}

/// Os pacotes: tudo em `<destdir>/usr`, apontando para `/usr`, sem avisar o systemd nem o D-Bus
/// (o "Pronto." só sai depois de avisá-los).
#[test]
fn destdir_stages_files_for_the_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let (bins, stage) = (fake_bins(dir.path()), dir.path().join("stage"));

    let out = install(&[
        "--prefix",
        "/usr",
        "--destdir",
        stage.to_str().unwrap(),
        "--from",
        bins.to_str().unwrap(),
    ]);

    for file in PROGRAMS
        .iter()
        .map(|p| format!("bin/{p}"))
        .chain(FILES.iter().map(|f| f.to_string()))
    {
        assert!(stage.join("usr").join(&file).is_file(), "{file}");
    }
    let unit = read(stage.join("usr").join(FILES[0]));
    assert!(
        unit.contains("ExecStart=\"/usr/bin/gdrive-desktop-linuxd\"\n"),
        "{unit}"
    );
    assert!(!out.contains("Pronto."), "{out}");
    // O pacote tem o marcador do formato no lugar da lista.
    assert!(
        !stage
            .join("usr/share/gdrive-desktop-linux/installed-files")
            .exists()
    );
}

/// O ícone do app virou PNG: o SVG de uma instalação anterior sai, senão o tema o prefere.
#[test]
fn removes_the_old_scalable_app_icon() {
    let dir = tempfile::tempdir().unwrap();
    let (bins, prefix) = (fake_bins(dir.path()), dir.path().join("pfx"));
    let old = prefix
        .join("share/icons/hicolor/scalable/apps/io.github.junior9283.GDriveDesktopLinux.svg");
    std::fs::create_dir_all(old.parent().unwrap()).unwrap();
    std::fs::write(&old, "<svg/>").unwrap();

    let args = [
        "--prefix",
        prefix.to_str().unwrap(),
        "--from",
        bins.to_str().unwrap(),
    ];
    install(&[args.as_slice(), &["--no-activate"]].concat());

    assert!(!old.exists());
}

/// Sem reler a configuração, o barramento da sessão não conhece a ativação recém-instalada e a
/// janela fica em "o serviço não está respondendo" até a próxima sessão.
#[test]
fn activation_reloads_systemd_and_the_session_bus() {
    let dir = tempfile::tempdir().unwrap();
    let (bins, prefix) = (fake_bins(dir.path()), dir.path().join("pfx"));
    // systemctl e busctl de mentira, que anotam a chamada em vez de mexer na sessão de verdade.
    let fakes = dir.path().join("comandos");
    let log = dir.path().join("chamadas");
    std::fs::create_dir_all(&fakes).unwrap();
    for command in ["systemctl", "busctl"] {
        let path = fakes.join(command);
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho {command} \"$@\" >> '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!("{}:{}", fakes.display(), std::env::var("PATH").unwrap());

    succeeded(run_with(
        &[
            "--prefix",
            prefix.to_str().unwrap(),
            "--from",
            bins.to_str().unwrap(),
        ],
        Some(&path),
    ));

    assert_eq!(
        read(log),
        "systemctl --user daemon-reload\n\
         busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig\n"
    );
}

#[test]
fn refuses_a_prefix_that_no_exec_line_can_hold() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a\"b", "a$b", "a%b", "a|b", "a&b", "a\\b", "a`b"] {
        let prefix = dir.path().join(name);
        let out = run(&["--prefix", prefix.to_str().unwrap(), "--dry-run"]);
        assert!(!out.status.success(), "{name}");
    }
}

#[test]
fn desktop_files_are_valid() {
    if Command::new("desktop-file-validate")
        .arg("--help")
        .output()
        .is_err()
    {
        eprintln!("pulado: falta o desktop-file-validate");
        return;
    }
    for entry in std::fs::read_dir(repo().join("packaging/desktop")).unwrap() {
        let path = entry.unwrap().path();
        let out = Command::new("desktop-file-validate")
            .arg(&path)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{}: {report}", path.display());
    }
}
