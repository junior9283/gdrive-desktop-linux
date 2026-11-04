//! A instalação para uso (`scripts/install-dev.sh`) e os arquivos de `packaging/`.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PROGRAMS: [&str; 5] = [
    "gdrive-desktop-linuxd",
    "gdrive-desktop-linux",
    "gdrive-desktop-linux-tray",
    "gdrive-desktop-linux-ui",
    "gdrive-desktop-linux-mcp",
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

/// Os plugins do Dolphin ficam em `<prefixo>/lib/qt6/plugins` e o Qt só os acha pelo
/// `QT_PLUGIN_PATH`: sem o arquivo em `environment.d` o menu e as nuvens somem sem aviso.
const PLUGINS: [&str; 2] = [
    "kfileitemaction/gdrive_desktop_linux_menu",
    "overlayicon/gdrive_desktop_linux_overlay",
];

#[test]
fn dolphin_plugins_install_with_the_qt_plugin_path() {
    let dir = tempfile::tempdir().unwrap();
    let (bins, prefix) = (fake_bins(dir.path()), dir.path().join("com espaço"));
    let (built, config) = (dir.path().join("plugins"), dir.path().join("config"));
    std::fs::create_dir_all(&built).unwrap();
    for plugin in PLUGINS {
        std::fs::write(
            built.join(format!("{}.so", plugin.rsplit('/').next().unwrap())),
            "",
        )
        .unwrap();
    }

    let out = Command::new("sh")
        .arg(repo().join("scripts/install-dev.sh"))
        .args(["--prefix", prefix.to_str().unwrap()])
        .args(["--from", bins.to_str().unwrap(), "--no-activate"])
        .args(["--dolphin", built.to_str().unwrap()])
        .env("XDG_CONFIG_HOME", &config)
        .output()
        .unwrap();
    succeeded(out);

    for plugin in PLUGINS {
        let file = prefix
            .join("lib/qt6/plugins/kf6")
            .join(format!("{plugin}.so"));
        assert!(file.is_file(), "{plugin}");
    }
    let conf = config.join("environment.d/50-gdrive-desktop-linux.conf");
    assert_eq!(
        read(conf.clone()),
        format!(
            "QT_PLUGIN_PATH={}/lib/qt6/plugins${{QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}}\n",
            prefix.display()
        )
    );
    let listed = read(prefix.join("share/gdrive-desktop-linux/installed-files"));
    assert!(
        listed.lines().any(|l| l == conf.display().to_string()),
        "{listed}"
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

/// Passa o `desktop-file-validate` em cada `.desktop` de `dir` (pulado sem o programa).
fn validate_desktop_dir(dir: &Path) {
    if Command::new("desktop-file-validate")
        .arg("--help")
        .output()
        .is_err()
    {
        eprintln!("pulado: falta o desktop-file-validate");
        return;
    }
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let out = Command::new("desktop-file-validate")
            .arg(&path)
            .output()
            .unwrap();
        let report = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{}: {report}", path.display());
    }
}

#[test]
fn desktop_files_are_valid() {
    validate_desktop_dir(&repo().join("packaging/desktop"));
}

/// O que `scripts/stage-manifest.sh` lista para a variante (`default` ou `gerasync`).
fn stage_manifest(flavor: &str) -> String {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("manifesto");
    let status = Command::new("sh")
        .arg(repo().join("scripts/stage-manifest.sh"))
        .args(["--flavor", flavor])
        .arg(&out)
        .status()
        .unwrap();
    assert!(status.success(), "stage-manifest.sh --flavor {flavor}");
    read(out)
}

/// A variante atual instala exatamente o que instalava antes do GeraSync: os mesmos caminhos e o
/// mesmo conteúdo (os programas entram só pelo caminho).
#[test]
fn default_stage_matches_golden() {
    let golden = read(repo().join("crates/gdl-common/tests/golden/default-stage.sha256"));
    assert_eq!(
        stage_manifest("default"),
        golden,
        "a instalação da variante atual mudou"
    );
}

/// Os três nomes da variante atual que não podem aparecer no GeraSync (fora o endereço do
/// repositório, que é o mesmo nas duas).
const OLD_NAMES: [&str; 3] = [
    "GDriveDesktopLinux",
    "gdrive-desktop-linux",
    "gdrive_desktop_linux",
];
const REPO_URL: &str = "github.com/junior9283/gdrive-desktop-linux";

/// A instalação do GeraSync em `<dir>/stage/usr`, como nos pacotes, com os programas de `bins`.
fn gerasync_stage(dir: &Path, bins: &Path) -> PathBuf {
    let stage = dir.join("stage");
    install(&[
        "--flavor",
        "gerasync",
        "--prefix",
        "/usr",
        "--destdir",
        stage.to_str().unwrap(),
        "--from",
        bins.to_str().unwrap(),
        "--no-dolphin",
    ]);
    stage.join("usr")
}

/// Todos os arquivos de `dir`, recursivamente.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for path in std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()) {
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found
}

fn names_old_app(text: &str) -> bool {
    let text = text.replace(REPO_URL, "");
    OLD_NAMES.iter().any(|name| text.contains(name))
}

#[test]
fn gerasync_stage_has_no_old_names() {
    let dir = tempfile::tempdir().unwrap();
    let usr = gerasync_stage(dir.path(), &fake_bins(dir.path()));

    let files = files_under(&usr);
    assert!(files.len() > 20, "{files:?}");
    for path in files {
        let relative = path.strip_prefix(&usr).unwrap().display().to_string();
        assert!(!names_old_app(&relative), "{relative}");
        if let Ok(text) = std::fs::read_to_string(&path) {
            assert!(!names_old_app(&text), "{relative}:\n{text}");
        }
    }
}

/// As duas variantes cabem no mesmo sistema: nenhum arquivo de uma sobrescreve o da outra.
#[test]
fn flavors_install_disjoint_files() {
    let paths = |manifest: String| -> std::collections::BTreeSet<String> {
        manifest
            .lines()
            .map(|l| l.split_once("  ").unwrap().1.to_owned())
            .collect()
    };
    let default = paths(stage_manifest("default"));
    let gerasync = paths(stage_manifest("gerasync"));
    assert!(gerasync.len() == default.len(), "{gerasync:?}");
    let shared: Vec<_> = default.intersection(&gerasync).collect();
    assert!(shared.is_empty(), "{shared:?}");
}

/// O que é do serviço do Google (os tipos dos ponteiros) e o endereço do projeto não mudam.
#[test]
fn gerasync_keeps_repo_url_and_mime_types() {
    let dir = tempfile::tempdir().unwrap();
    let usr = gerasync_stage(dir.path(), &fake_bins(dir.path()));

    let unit = read(usr.join("share/systemd/user/gerasyncd.service"));
    assert!(
        unit.contains("Documentation=https://github.com/junior9283/gdrive-desktop-linux\n"),
        "{unit}"
    );
    assert!(
        unit.contains("ExecStart=\"/usr/bin/gerasyncd\"\n"),
        "{unit}"
    );
    let mime = read(usr.join("share/mime/packages/gerasync.xml"));
    assert!(mime.contains("application/x-gdrive-document"), "{mime}");
    assert!(
        mime.contains("<comment>Google Drive link</comment>"),
        "{mime}"
    );
    let desktop = read(usr.join("share/applications/io.github.junior9283.GeraSync.desktop"));
    assert!(desktop.contains("Name=GeraSync\n"), "{desktop}");
    assert!(
        desktop.contains("Icon=io.github.junior9283.GeraSync\n"),
        "{desktop}"
    );
    assert!(
        usr.join("share/icons/hicolor/48x48/apps/io.github.junior9283.GeraSync.png")
            .is_file()
    );
}

#[test]
fn gerasync_desktop_files_are_valid() {
    let dir = tempfile::tempdir().unwrap();
    let usr = gerasync_stage(dir.path(), &fake_bins(dir.path()));
    validate_desktop_dir(&usr.join("share/applications"));
}

/// Roda o `package.sh` com `args` e um nfpm de mentira, que guarda a configuração de cada formato
/// em `<saída>/<formato>.yaml` e cria um pacote vazio com o nome que o nfpm dá (versão 0).
fn package(dir: &Path, args: &[&str], out: &Path) {
    let nfpm = dir.join("nfpm");
    std::fs::write(
        &nfpm,
        "#!/bin/sh\n\
         while [ $# -gt 0 ]; do\n\
           case \"$1\" in --config) c=$2 ;; --packager) p=$2 ;; --target) t=$2 ;; esac; shift\n\
         done\n\
         name=$(sed -n 's/^name: //p' \"$c\")\n\
         cp \"$c\" \"$t/$p.yaml\"\n\
         case $p in deb) f=${name}_0_amd64.deb ;; rpm) f=$name-0-1.x86_64.rpm ;;\n\
           *) f=$name-0-1-x86_64.pkg.tar.zst ;; esac\n\
         : > \"$t/$f\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&nfpm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let status = Command::new("sh")
        .arg(repo().join("scripts/package.sh"))
        .args(args)
        .arg(out)
        .env("NFPM", &nfpm)
        .status()
        .unwrap();
    assert!(status.success(), "package.sh {args:?}");
}

/// Os `.tar.gz` em `out`.
fn tarballs(out: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".tar.gz"))
        .collect()
}

/// As duas variantes saem na mesma pasta do release: o GeraSync com nome e página dele, e o
/// SHA256SUMS com os arquivos das duas.
#[test]
fn both_flavors_package_into_the_same_folder() {
    let dir = tempfile::tempdir().unwrap();
    let bins = fake_bins(dir.path());
    let out = dir.path().join("dist");
    std::fs::create_dir_all(&out).unwrap();
    let from = bins.to_str().unwrap();

    package(dir.path(), &["--from", from], &out);
    let default_deb = read(out.join("deb.yaml"));
    package(dir.path(), &["--flavor", "gerasync", "--from", from], &out);

    assert!(default_deb.contains("name: gdrive-desktop-linux\n"));
    let deb = read(out.join("deb.yaml"));
    assert!(deb.contains("name: gerasync\n"), "{deb}");
    assert!(
        deb.contains("homepage: https://junior9283.github.io/gerasync/\n"),
        "{deb}"
    );
    assert!(
        deb.contains("Google Drive"),
        "a descrição diz o que o app é:\n{deb}"
    );
    assert!(deb.contains("dst: \"/usr/bin/gerasyncd\""), "{deb}");
    assert!(
        deb.contains("dst: \"/usr/share/gerasync/package\""),
        "{deb}"
    );
    let sums = read(out.join("SHA256SUMS"));
    for file in [
        "gdrive-desktop-linux_0_amd64.deb",
        "gerasync_0_amd64.deb",
        "gerasync-0-1.x86_64.rpm",
        "gerasync-0-1-x86_64.pkg.tar.zst",
    ] {
        assert_eq!(
            sums.matches(&format!("  {file}\n")).count(),
            1,
            "{file}:\n{sums}"
        );
    }
    let tarballs = tarballs(&out);
    assert_eq!(tarballs.len(), 2, "{tarballs:?}");
    assert!(tarballs.iter().any(|p| {
        p.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("gerasync-")
    }));
}

/// O `.tar.gz` do GeraSync instala o GeraSync: o install.sh dele passa a variante.
#[test]
fn gerasync_tarball_installs_gerasync() {
    let dir = tempfile::tempdir().unwrap();
    let bins = fake_bins(dir.path());
    let out = dir.path().join("dist");
    std::fs::create_dir_all(&out).unwrap();
    package(
        dir.path(),
        &["--flavor", "gerasync", "--from", bins.to_str().unwrap()],
        &out,
    );
    let tarball = tarballs(&out).pop().unwrap();
    let unpacked = dir.path().join("unpacked");
    std::fs::create_dir_all(&unpacked).unwrap();
    let status = Command::new("tar")
        .args(["-xzf", tarball.to_str().unwrap(), "-C"])
        .arg(&unpacked)
        .status()
        .unwrap();
    assert!(status.success());
    let root = std::fs::read_dir(&unpacked)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let prefix = dir.path().join("pfx");

    let out = Command::new("sh")
        .arg(root.join("install.sh"))
        .args([
            "--prefix",
            prefix.to_str().unwrap(),
            "--no-activate",
            "--no-dolphin",
        ])
        .output()
        .unwrap();
    succeeded(out);

    assert!(prefix.join("bin/gerasync-tray").is_file());
    assert!(prefix.join("share/gerasync/installed-files").is_file());
    let uninstall = read(root.join("uninstall.sh"));
    assert!(
        uninstall.contains("/bin/gerasync\" uninstall"),
        "{uninstall}"
    );
}
