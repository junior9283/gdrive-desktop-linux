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
    "share/mime/packages/gdrive-desktop-linux.xml",
    "share/icons/hicolor/48x48/apps/io.github.junior9283.GDriveDesktopLinux.png",
    "share/icons/hicolor/256x256/apps/io.github.junior9283.GDriveDesktopLinux.png",
    "share/gdrive-desktop-linux/icons/hicolor/scalable/status/gdl-synced.svg",
    "share/gdrive-desktop-linux/icons/hicolor/scalable/status/gdl-syncing-4.svg",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str]) -> Output {
    Command::new("sh")
        .arg(repo().join("scripts/install-dev.sh"))
        .args(args)
        .output()
        .unwrap()
}

/// Roda o `install-dev.sh` com `args` e devolve o que ele escreveu.
fn install(args: &[&str]) -> String {
    let out = run(args);
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
    ];
    for (file, line) in wanted {
        let body = read(prefix.join(file));
        assert!(body.contains(&line), "{file}: {line}\n{body}");
    }
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
