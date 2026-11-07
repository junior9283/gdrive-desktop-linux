//! O que oferecer de uma release nova: a versão e, quando a release traz os três, os arquivos
//! que a atualização automática precisa (o pacote desta variante e formato, as somas e a
//! assinatura delas).

use crate::updates::{Asset, Release, Version};

use super::InstallKind;

/// As somas dos pacotes do release e a assinatura minisign delas.
pub const SUMS: &str = "SHA256SUMS";
pub const SIGNATURE: &str = "SHA256SUMS.minisig";

/// Os arquivos para instalar sozinho.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseFiles {
    pub package: Asset,
    pub sums: Asset,
    pub signature: Asset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: Version,
    /// `None`: falta algum dos três (ou a instalação não tem pacote no release); a oferta vira
    /// só "Ver novidades".
    pub files: Option<ReleaseFiles>,
}

/// A oferta de `release` para uma instalação `kind`.
pub fn offer(release: Release, kind: &InstallKind) -> Offer {
    let package = kind.package_name(release.version);
    let mut assets = release.assets;
    let mut take = |name: &str| {
        let at = assets.iter().position(|a| a.name == name)?;
        Some(assets.swap_remove(at))
    };
    let files = (|| {
        Some(ReleaseFiles {
            package: take(package.as_deref()?)?,
            sums: take(SUMS)?,
            signature: take(SIGNATURE)?,
        })
    })();
    Offer {
        version: release.version,
        files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    const V: Version = (1, 4, 0);

    fn release(names: &[&str]) -> Release {
        let base = Url::parse("https://example.invalid/download/").unwrap();
        Release {
            version: V,
            assets: names
                .iter()
                .map(|name| Asset {
                    name: name.to_string(),
                    url: base.join(name).unwrap(),
                })
                .collect(),
        }
    }

    #[test]
    fn offer_with_all_files_can_install() {
        let deb = InstallKind::Deb.package_name(V).unwrap();
        let offer = offer(release(&[&deb, SUMS, SIGNATURE]), &InstallKind::Deb);
        let files = offer.files.expect("os três arquivos");
        assert_eq!(files.package.name, deb);
        assert_eq!(files.sums.name, SUMS);
        assert_eq!(files.signature.name, SIGNATURE);
    }

    #[test]
    fn offer_without_package_cannot_install() {
        let missing = offer(release(&[SUMS, SIGNATURE]), &InstallKind::Rpm);
        assert_eq!(
            missing,
            Offer {
                version: V,
                files: None
            }
        );
        let rpm = InstallKind::Rpm.package_name(V).unwrap();
        let unsigned = offer(release(&[&rpm, SUMS]), &InstallKind::Rpm);
        assert_eq!(unsigned.files, None, "sem assinatura não instala");
        let source = offer(release(&[&rpm, SUMS, SIGNATURE]), &InstallKind::Source);
        assert_eq!(source.files, None);
    }

    #[test]
    fn offer_ignores_other_flavor() {
        let other = if gdl_common::brand::OFFICIAL {
            "gdrive-desktop-linux_1.4.0_amd64.deb"
        } else {
            "gerasync_1.4.0_amd64.deb"
        };
        let offer = offer(release(&[other, SUMS, SIGNATURE]), &InstallKind::Deb);
        assert_eq!(offer.files, None);
    }
}
