//! UDisks2 falso, para os testes do serviço e o serviço dos testes de ponta a ponta: cada
//! dispositivo é uma unidade removível (`Drive`) e um sistema de arquivos montado (`Block` e
//! `Filesystem`), publicados sob um `org.freedesktop.DBus.ObjectManager` em `UDISKS_PATH`, que
//! avisa a entrada e a saída.

use std::path::{Path, PathBuf};

use zbus::ObjectServer;
use zbus::zvariant::OwnedObjectPath;

use crate::devices::UDISKS_PATH;

struct FakeUDisksDrive;

#[zbus::interface(name = "org.freedesktop.UDisks2.Drive")]
impl FakeUDisksDrive {
    #[zbus(property)]
    fn removable(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn media_removable(&self) -> bool {
        true
    }
}

struct FakeBlock {
    uuid: String,
    label: String,
    drive: OwnedObjectPath,
}

#[zbus::interface(name = "org.freedesktop.UDisks2.Block")]
impl FakeBlock {
    #[zbus(property, name = "IdUUID")]
    fn id_uuid(&self) -> String {
        self.uuid.clone()
    }

    #[zbus(property, name = "IdLabel")]
    fn id_label(&self) -> String {
        self.label.clone()
    }

    #[zbus(property)]
    fn size(&self) -> u64 {
        1 << 30
    }

    #[zbus(property)]
    fn hint_system(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn drive(&self) -> OwnedObjectPath {
        self.drive.clone()
    }
}

struct FakeFilesystem {
    mount: PathBuf,
}

#[zbus::interface(name = "org.freedesktop.UDisks2.Filesystem")]
impl FakeFilesystem {
    /// Caminhos terminados em NUL, como o UDisks2 manda.
    #[zbus(property)]
    fn mount_points(&self) -> Vec<Vec<u8>> {
        use std::os::unix::ffi::OsStrExt;
        let mut path = self.mount.as_os_str().as_bytes().to_vec();
        path.push(0);
        vec![path]
    }
}

/// Conecta, em `server`, um pendrive montado em `mount`.
pub async fn plug(
    server: &ObjectServer,
    uuid: &str,
    label: &str,
    mount: &Path,
) -> zbus::Result<()> {
    let (drive, block) = paths(uuid);
    server.at(drive.clone(), FakeUDisksDrive).await?;
    let mount = mount.to_path_buf();
    server.at(block.clone(), FakeFilesystem { mount }).await?;
    let drive = OwnedObjectPath::try_from(drive)?;
    let (uuid, label) = (uuid.into(), label.into());
    server.at(block, FakeBlock { uuid, label, drive }).await?;
    Ok(())
}

/// Desconecta o pendrive `uuid`.
pub async fn unplug(server: &ObjectServer, uuid: &str) -> zbus::Result<()> {
    let (drive, block) = paths(uuid);
    server.remove::<FakeBlock, _>(block.clone()).await?;
    server.remove::<FakeFilesystem, _>(block).await?;
    server.remove::<FakeUDisksDrive, _>(drive).await?;
    Ok(())
}

/// Os objetos da unidade e do sistema de arquivos do pendrive `uuid`.
fn paths(uuid: &str) -> (String, String) {
    let name = uuid.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
    (
        format!("{UDISKS_PATH}/drives/d{name}"),
        format!("{UDISKS_PATH}/block_devices/b{name}"),
    )
}
