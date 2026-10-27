//! Dispositivos removíveis (pendrive, cartão, câmera) pelo UDisks2, no barramento do sistema, e
//! as escolhas do usuário para cada um (`<data_dir>/devices`).

use std::collections::BTreeMap;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use zbus::fdo::ManagedObjects;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub const UDISKS_SERVICE: &str = "org.freedesktop.UDisks2";
pub const UDISKS_PATH: &str = "/org/freedesktop/UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
pub const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
pub const DRIVE: &str = "org.freedesktop.UDisks2.Drive";

/// Um sistema de arquivos montado de uma unidade removível.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// O UUID do sistema de arquivos; sem ele, rótulo e tamanho.
    pub id: String,
    pub label: String,
    pub mount: PathBuf,
}

/// Os sistemas de arquivos montados de unidades removíveis (`Removable` ou `MediaRemovable`, sem
/// `HintSystem`) entre os objetos do UDisks2.
pub fn mounted_removables(objects: &ManagedObjects) -> Vec<Device> {
    let mut found: Vec<Device> = objects
        .keys()
        .filter_map(|path| device_of(objects, path))
        .collect();
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// A propriedade `name` da interface `iface` do objeto `path`, no tipo pedido.
fn get<T: TryFrom<OwnedValue>>(
    objects: &ManagedObjects,
    path: &OwnedObjectPath,
    iface: &str,
    name: &str,
) -> Option<T> {
    let (_, props) = objects
        .get(path)?
        .iter()
        .find(|(i, _)| i.as_str() == iface)?;
    T::try_from(props.get(name)?.try_clone().ok()?).ok()
}

/// O objeto `path`, se ele é um sistema de arquivos montado de unidade removível.
fn device_of(objects: &ManagedObjects, path: &OwnedObjectPath) -> Option<Device> {
    let flag = |path, iface, name| get::<bool>(objects, path, iface, name).unwrap_or(false);
    let text = |name| get::<String>(objects, path, BLOCK, name).unwrap_or_default();
    let mounts: Vec<Vec<u8>> = get(objects, path, FILESYSTEM, "MountPoints")?;
    let mount = mounts.into_iter().next()?;
    let drive: OwnedObjectPath = get(objects, path, BLOCK, "Drive")?;
    let removable = flag(&drive, DRIVE, "Removable") || flag(&drive, DRIVE, "MediaRemovable");
    if !removable || flag(path, BLOCK, "HintSystem") {
        return None;
    }
    let mount = PathBuf::from(std::ffi::OsStr::from_bytes(
        mount.strip_suffix(&[0]).unwrap_or(&mount),
    ));
    let mut label = text("IdLabel");
    if label.trim().is_empty() {
        label = crate::file_name(&mount);
    }
    let mut id = text("IdUUID");
    if id.is_empty() {
        let size: u64 = get(objects, path, BLOCK, "Size").unwrap_or(0);
        id = format!("{label}-{size}");
    }
    Some(Device {
        id: clean(&id),
        label: clean(&label),
        mount,
    })
}

/// Sem tabulação nem quebra de linha: o arquivo de escolhas é uma linha por dispositivo.
fn clean(text: &str) -> String {
    text.replace(['\t', '\n', '\r'], " ")
}

/// O que o usuário escolheu para um dispositivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    /// Fazer backup sozinho sempre que ele for conectado.
    Backup,
    /// Não perguntar mais.
    Ignored,
}

impl Choice {
    pub fn as_str(self) -> &'static str {
        match self {
            Choice::Backup => "backup",
            Choice::Ignored => "ignored",
        }
    }

    fn parse(text: &str) -> Option<Choice> {
        match text {
            "backup" => Some(Choice::Backup),
            "ignored" => Some(Choice::Ignored),
            _ => None,
        }
    }
}

/// As escolhas guardadas: `id \t escolha \t rótulo` por linha.
#[derive(Debug, Default)]
pub struct Choices {
    path: PathBuf,
    /// ID → (escolha, rótulo).
    map: BTreeMap<String, (Choice, String)>,
}

impl Choices {
    /// Lê o arquivo; linhas que não entende ficam de fora.
    pub fn load(path: PathBuf) -> Choices {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let map = text
            .lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                let (id, choice) = (parts.next()?, Choice::parse(parts.next()?)?);
                Some((
                    id.to_owned(),
                    (choice, parts.next().unwrap_or_default().to_owned()),
                ))
            })
            .collect();
        Choices { path, map }
    }

    pub fn choice(&self, id: &str) -> Option<Choice> {
        self.map.get(id).map(|(choice, _)| *choice)
    }

    /// ID e rótulo de cada dispositivo com escolha guardada.
    pub fn labels(&self) -> impl Iterator<Item = (&String, &String)> {
        self.map.iter().map(|(id, (_, label))| (id, label))
    }

    pub fn set(&mut self, id: &str, choice: Choice, label: &str) -> io::Result<()> {
        self.map.insert(id.to_owned(), (choice, clean(label)));
        self.save()
    }

    /// Esquece a escolha; `false` se não havia.
    pub fn forget(&mut self, id: &str) -> io::Result<bool> {
        if self.map.remove(id).is_none() {
            return Ok(false);
        }
        self.save().map(|()| true)
    }

    fn save(&self) -> io::Result<()> {
        let text: String = self
            .map
            .iter()
            .map(|(id, (choice, label))| format!("{id}\t{}\t{label}\n", choice.as_str()))
            .collect();
        gdl_common::fs::write_atomic(&self.path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zbus::names::OwnedInterfaceName;
    use zbus::zvariant::Value;

    use super::*;

    type Props = Vec<(&'static str, Value<'static>)>;

    fn objects(list: Vec<(&str, Vec<(&str, Props)>)>) -> ManagedObjects {
        list.into_iter()
            .map(|(path, ifaces)| {
                let ifaces = ifaces
                    .into_iter()
                    .map(|(iface, props)| {
                        let props: HashMap<String, OwnedValue> = props
                            .into_iter()
                            .map(|(k, v)| (k.to_owned(), v.try_into().unwrap()))
                            .collect();
                        (OwnedInterfaceName::try_from(iface).unwrap(), props)
                    })
                    .collect();
                (OwnedObjectPath::try_from(path).unwrap(), ifaces)
            })
            .collect()
    }

    fn drive(path: &str, removable: bool) -> (&str, Vec<(&str, Props)>) {
        (
            path,
            vec![(
                DRIVE,
                vec![
                    ("Removable", Value::from(removable)),
                    ("MediaRemovable", Value::from(false)),
                ],
            )],
        )
    }

    fn block<'a>(
        path: &'a str,
        drive: &str,
        uuid: &str,
        label: &str,
        mount: &str,
        system: bool,
    ) -> (&'a str, Vec<(&'a str, Props)>) {
        let mounts: Vec<Vec<u8>> = if mount.is_empty() {
            vec![]
        } else {
            vec![format!("{mount}\0").into_bytes()]
        };
        (
            path,
            vec![
                (
                    BLOCK,
                    vec![
                        ("IdUUID", Value::from(uuid.to_owned())),
                        ("IdLabel", Value::from(label.to_owned())),
                        ("Size", Value::from(1024u64)),
                        ("HintSystem", Value::from(system)),
                        (
                            "Drive",
                            Value::from(OwnedObjectPath::try_from(drive).unwrap()),
                        ),
                    ],
                ),
                (FILESYSTEM, vec![("MountPoints", Value::from(mounts))]),
            ],
        )
    }

    #[test]
    fn only_mounted_removable_filesystems_count() {
        let found = mounted_removables(&objects(vec![
            drive("/d/usb", true),
            drive("/d/ssd", false),
            block(
                "/b/usb1",
                "/d/usb",
                "AAAA",
                "FOTOS",
                "/run/media/u/FOTOS",
                false,
            ),
            block("/b/usb2", "/d/usb", "BBBB", "OUTRA", "", false),
            block("/b/ssd1", "/d/ssd", "CCCC", "sistema", "/", false),
            block("/b/usb3", "/d/usb", "DDDD", "EFI", "/boot/efi", true),
            block(
                "/b/usb4",
                "/d/usb",
                "",
                "",
                "/run/media/u/sem\trotulo",
                false,
            ),
        ]));

        assert_eq!(
            found,
            vec![
                Device {
                    id: "AAAA".into(),
                    label: "FOTOS".into(),
                    mount: "/run/media/u/FOTOS".into(),
                },
                Device {
                    id: "sem rotulo-1024".into(),
                    label: "sem rotulo".into(),
                    mount: "/run/media/u/sem\trotulo".into(),
                },
            ]
        );
    }

    #[test]
    fn choices_survive_a_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("dados/devices");
        let mut choices = Choices::load(path.clone());
        choices.set("AAAA", Choice::Backup, "FOTOS").unwrap();
        choices.set("BBBB", Choice::Ignored, "CÂMERA").unwrap();
        assert!(choices.forget("AAAA").unwrap());
        assert!(!choices.forget("AAAA").unwrap());
        choices.set("CCCC", Choice::Backup, "com\ttab").unwrap();

        let again = Choices::load(path);
        assert_eq!(
            again.map.into_iter().collect::<Vec<_>>(),
            vec![
                ("BBBB".into(), (Choice::Ignored, "CÂMERA".into())),
                ("CCCC".into(), (Choice::Backup, "com tab".into())),
            ]
        );
    }
}
