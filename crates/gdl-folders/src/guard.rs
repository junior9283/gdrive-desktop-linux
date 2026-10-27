//! Proteções: que pastas podem ser sincronizadas, quando um par para e quando uma comparação
//! pergunta antes de mandar coisas para a lixeira.

use std::io;
use std::path::{Path, PathBuf};

use gdl_store::folders::StopReason;

/// Acima disso, uma comparação pergunta antes de excluir (de qualquer tamanho de pasta).
pub const MASS_DELETE: usize = 1000;
/// A regra da metade só vale a partir deste tanto: apagar 3 de 4 arquivos não é acidente.
const HALF_RULE_MIN: usize = 20;

/// Por que uma pasta não pode ser sincronizada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Missing,
    Unreadable,
    /// Dentro de uma pasta já sincronizada (ou da montagem, ou dos dados do app).
    Nested,
    /// Contém uma pasta já sincronizada (ou a montagem, ou os dados do app).
    Contains,
    /// Volume de rede ou FUSE.
    Network,
}

/// O que o configurador mostra de uma pasta aceita.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FolderInfo {
    pub size: u64,
    pub files: u64,
}

/// Confere `path` contra as pastas já em uso (`taken`: pares, montagens, pasta de dados) e o
/// tipo de volume (`is_network`, injetável nos testes); devolve tamanho e contagem.
pub fn check_folder(
    path: &Path,
    taken: &[PathBuf],
    is_network: impl Fn(&Path) -> bool,
) -> Result<FolderInfo, Refusal> {
    let path = path.canonicalize().map_err(|_| Refusal::Missing)?;
    if !path.is_dir() {
        return Err(Refusal::Missing);
    }
    if let Some(refusal) = overlap(&path, taken) {
        return Err(refusal);
    }
    if is_network(&path) {
        return Err(Refusal::Network);
    }
    std::fs::read_dir(&path).map_err(|_| Refusal::Unreadable)?;
    let mut info = FolderInfo::default();
    for entry in walkdir::WalkDir::new(&path)
        .min_depth(1)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() {
            info.files += 1;
            info.size += entry.metadata().map_or(0, |m| m.len());
        }
    }
    Ok(info)
}

/// `path` (canônico) é uma das pastas `taken`, está dentro de uma ou contém uma.
pub fn overlap(path: &Path, taken: &[PathBuf]) -> Option<Refusal> {
    taken.iter().find_map(|other| {
        let other = other.canonicalize().unwrap_or_else(|_| other.clone());
        if path.starts_with(&other) {
            Some(Refusal::Nested)
        } else {
            other.starts_with(path).then_some(Refusal::Contains)
        }
    })
}

/// O volume de `path` é de rede (NFS, SMB/CIFS, AFS, 9P, Ceph) ou FUSE (sshfs e outros).
pub fn network_fs(path: &Path) -> bool {
    const NETWORK: [u64; 8] = [
        0x6969,      // NFS
        0x517B,      // SMB
        0xFF53_4D42, // CIFS
        0xFE53_4D42, // SMB2
        0x6573_5546, // FUSE
        0x5346_414F, // AFS
        0x0102_1997, // 9P
        0x00C3_6400, // Ceph
    ];
    rustix::fs::statfs(path).is_ok_and(|s| NETWORK.contains(&(s.f_type as u64)))
}

/// A raiz do par não está utilizável: o par para (nunca vira "tudo excluído").
pub fn root_problem(root: &Path) -> Option<StopReason> {
    match std::fs::read_dir(root) {
        Ok(_) => None,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Some(StopReason::NoPermission),
        Err(_) => Some(StopReason::Missing),
    }
}

/// `deletes` exclusões de uma vez num par com `total` caminhos sincronizados pedem confirmação.
pub fn is_mass_delete(deletes: usize, total: usize) -> bool {
    deletes > MASS_DELETE || (deletes >= HALF_RULE_MIN && deletes * 2 > total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(root: &Path, rel: &str) -> PathBuf {
        let path = root.join(rel);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn nested_folders_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let docs = folder(tmp.path(), "Documentos");
        let inner = folder(tmp.path(), "Documentos/Notas");
        let free = folder(tmp.path(), "Imagens");
        std::fs::write(free.join("a.png"), b"1234").unwrap();
        let local = |_: &Path| false;

        assert_eq!(
            check_folder(&inner, std::slice::from_ref(&docs), local),
            Err(Refusal::Nested)
        );
        assert_eq!(
            check_folder(tmp.path(), std::slice::from_ref(&docs), local),
            Err(Refusal::Contains)
        );
        assert_eq!(
            check_folder(&docs, std::slice::from_ref(&docs), local),
            Err(Refusal::Nested)
        );
        assert_eq!(
            check_folder(&tmp.path().join("nada"), &[], local),
            Err(Refusal::Missing)
        );
        assert_eq!(
            check_folder(&free, &[docs], local),
            Ok(FolderInfo { size: 4, files: 1 })
        );
    }

    #[test]
    fn network_volume_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let share = folder(tmp.path(), "rede");
        let network = |p: &Path| p.ends_with("rede");

        assert_eq!(check_folder(&share, &[], network), Err(Refusal::Network));
        assert!(!network_fs(tmp.path()), "a pasta temporária é local");
    }

    #[test]
    fn mass_delete_needs_many_or_most() {
        assert!(
            !is_mass_delete(3, 3),
            "pasta pequena esvaziada não pergunta"
        );
        assert!(is_mass_delete(20, 30));
        assert!(!is_mass_delete(20, 50));
        assert!(is_mass_delete(1001, 1_000_000));
        assert!(!is_mass_delete(1000, 1_000_000));
    }
}
