//! Aviso de nova versão: o serviço consulta a última release do projeto 1 min depois da partida
//! e a cada 24 h; uma versão maior que a instalada vira um aviso, uma vez por versão.

use std::time::Duration;

use url::Url;

/// `(maior, menor, correção)`: compara na ordem numérica.
pub type Version = (u64, u64, u64);

/// Onde e quando consultar a última release.
pub struct UpdateCheck {
    pub url: Url,
    /// Espera depois da partida.
    pub first: Duration,
    pub every: Duration,
}

impl UpdateCheck {
    pub fn github() -> UpdateCheck {
        UpdateCheck {
            url: Url::parse(gdl_common::links::LATEST_RELEASE_API).expect("endereço fixo"),
            first: Duration::from_secs(60),
            every: Duration::from_secs(24 * 60 * 60),
        }
    }
}

/// `vX.Y.Z` ou `X.Y.Z`; qualquer outra forma (pré-lançamento incluso) não é versão.
pub fn version(tag: &str) -> Option<Version> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let mut next = || parts.next()?.parse().ok();
    let found = (next()?, next()?, next()?);
    parts.next().is_none().then_some(found)
}

/// A versão deste programa.
pub fn installed() -> Version {
    version(env!("CARGO_PKG_VERSION")).expect("versão do pacote")
}

/// `X.Y.Z`.
pub fn text(version: Version) -> String {
    format!("{}.{}.{}", version.0, version.1, version.2)
}

/// A versão da última release em `url`; `None` quando a *tag* não é uma versão.
pub async fn latest(url: &Url) -> Result<Option<Version>, gdl_drive::HttpError> {
    #[derive(serde::Deserialize)]
    struct Release {
        tag_name: String,
    }
    let release: Release = gdl_drive::fetch_json(url).await?;
    Ok(version(&release.tag_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Harness;

    const NEW: &str = "Nova versão disponível";

    fn announced(h: &Harness) -> Vec<crate::testutil::Note> {
        h.notes().into_iter().filter(|n| n.summary == NEW).collect()
    }

    #[test]
    fn versions_are_read_from_tags() {
        assert_eq!(version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(version("0.10.0"), Some((0, 10, 0)));
        for tag in ["v1.2", "v1.2.3-rc1", "nightly", "v1.2.3.4", ""] {
            assert_eq!(version(tag), None, "{tag}");
        }
        assert!(
            version("v0.10.0") > version("v0.9.9"),
            "numérico, não texto"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn newer_release_is_announced_once() {
        let mut h = Harness::new().await;
        h.fake.set_latest_release(Some("v99.0.0"));

        h.daemon.check_release().await;
        h.daemon.check_release().await;
        h.restart(None).await;
        h.daemon.check_release().await;

        let notes = announced(&h);
        assert_eq!(notes.len(), 1, "uma vez por versão: {notes:?}");
        assert!(notes[0].body.contains("99.0.0"), "{notes:?}");
        assert!(notes[0].actions.iter().any(|a| a == "open-release"));
        h.fake.set_latest_release(Some("v99.0.1"));
        h.daemon.check_release().await;
        assert_eq!(announced(&h).len(), 2, "a versão seguinte avisa de novo");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn same_or_older_release_is_silent() {
        let h = Harness::new().await;
        for tag in [concat!("v", env!("CARGO_PKG_VERSION")), "v0.0.1"] {
            h.fake.set_latest_release(Some(tag));
            h.daemon.check_release().await;
        }
        assert!(announced(&h).is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn release_check_failure_is_quiet() {
        let h = Harness::new().await;
        h.fake.set_latest_release(None);
        h.daemon.check_release().await;
        h.fake.set_latest_release(Some("nightly"));
        h.daemon.check_release().await;
        h.fake.inject(gdl_fake_drive::Fault::Offline);
        h.daemon.check_release().await;
        assert!(h.notes().is_empty(), "{:?}", h.notes());
    }
}
