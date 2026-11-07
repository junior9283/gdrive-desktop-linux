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
    /// Entre consultas, e entre um aviso e outro da mesma versão.
    pub every: Duration,
    /// A chave minisign que assina o `SHA256SUMS`; vazia, o app não se atualiza sozinho.
    pub public_key: String,
}

impl UpdateCheck {
    pub fn github() -> UpdateCheck {
        UpdateCheck {
            url: Url::parse(gdl_common::links::LATEST_RELEASE_API).expect("endereço fixo"),
            first: Duration::from_secs(60),
            every: Duration::from_secs(24 * 60 * 60),
            public_key: gdl_common::links::RELEASE_PUBLIC_KEY.into(),
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

/// Um arquivo do release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: Url,
}

/// A última release: a versão e os arquivos dela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub assets: Vec<Asset>,
}

/// A última release em `url`; `None` quando a *tag* não é uma versão.
pub async fn latest(url: &Url) -> Result<Option<Release>, gdl_drive::HttpError> {
    #[derive(serde::Deserialize)]
    struct Json {
        tag_name: String,
        #[serde(default)]
        assets: Vec<JsonAsset>,
    }
    #[derive(serde::Deserialize)]
    struct JsonAsset {
        name: String,
        browser_download_url: String,
    }
    let release: Json = gdl_drive::fetch_json(url).await?;
    Ok(version(&release.tag_name).map(|version| Release {
        version,
        assets: release
            .assets
            .into_iter()
            .filter_map(|a| {
                let url = Url::parse(&a.browser_download_url).ok()?;
                Some(Asset { name: a.name, url })
            })
            .collect(),
    }))
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
    async fn latest_reads_the_release_files() {
        let h = Harness::new().await;
        h.fake
            .set_release(Some("v1.4.0"), vec![("SHA256SUMS".into(), b"x".to_vec())]);
        let release = latest(&h.fake.releases_url()).await.unwrap().unwrap();
        assert_eq!(release.version, (1, 4, 0));
        assert_eq!(release.assets.len(), 1);
        assert_eq!(release.assets[0].name, "SHA256SUMS");
        let body = gdl_drive::fetch_bytes(&release.assets[0].url)
            .await
            .unwrap();
        assert_eq!(body, b"x");
    }

    use crate::daemon::releases::{DOWNLOAD_DIR, STATE_FILE};
    use crate::daemon::{Exit, UpdateStep};
    use crate::notify::{LATER, START_UPDATE};
    use crate::testutil::wait_until;
    use crate::upgrade::InstallKind;
    use crate::upgrade::install::InstallOutcome;
    use crate::upgrade::offer::{SIGNATURE, SUMS};
    use crate::upgrade::verify::sha256_hex;
    use crate::upgrade::verify::tests::{sign_for_harness, sums_of};

    const FAILED: &str = "Não deu para atualizar";
    const UPDATED: &str = "Atualizado para a versão";

    /// O `.deb` desta variante na versão `tag`.
    fn deb(tag: &str) -> String {
        InstallKind::Deb
            .package_name(version(tag).unwrap())
            .unwrap()
    }

    /// Publica a release `tag` com o pacote `served`, as somas (do conteúdo `listed`) e a
    /// assinatura da chave que o `Harness` confia.
    fn publish(h: &Harness, tag: &str, listed: &[u8], served: &[u8]) {
        let sums = sums_of(&[(&deb(tag), listed)]);
        let signature = sign_for_harness(&sums);
        h.fake.set_release(
            Some(tag),
            vec![
                (deb(tag), served.to_vec()),
                (SUMS.into(), sums),
                (SIGNATURE.into(), signature),
            ],
        );
    }

    fn named(h: &Harness, summary: &str) -> Vec<crate::testutil::Note> {
        h.notes()
            .into_iter()
            .filter(|n| n.summary.starts_with(summary))
            .collect()
    }

    fn saved(h: &Harness) -> serde_json::Value {
        let file = h.paths().state_dir.join(STATE_FILE);
        serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
    }

    fn write_saved(h: &Harness, state: serde_json::Value) {
        std::fs::create_dir_all(&h.paths().state_dir).unwrap();
        std::fs::write(h.paths().state_dir.join(STATE_FILE), state.to_string()).unwrap();
    }

    fn download_dir(h: &Harness) -> std::path::PathBuf {
        h.paths().cache_dir.join(DOWNLOAD_DIR)
    }

    /// A release `v99.0.0` assinada, já consultada (a oferta instalável).
    async fn offered(h: &Harness) {
        publish(h, "v99.0.0", b"p", b"p");
        h.daemon.check_release().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn offer_has_update_now_and_later() {
        let h = Harness::new().await;
        publish(&h, "v99.0.0", b"pacote", b"pacote");

        h.daemon.check_release().await;

        let notes = announced(&h);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].body.contains("99.0.0"), "{notes:?}");
        let actions = &notes[0].actions;
        assert!(actions.iter().any(|a| a == START_UPDATE), "{actions:?}");
        assert!(actions.iter().any(|a| a == LATER), "{actions:?}");
        let (offer, step) = h.daemon.update_state();
        assert_eq!(offer.unwrap().version, (99, 0, 0));
        assert_eq!(step, UpdateStep::Available);
    }

    /// Sem a assinatura (ou sem pacote para esta instalação), só "Ver novidades".
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn unsigned_release_only_opens_the_page() {
        let h = Harness::new().await;
        h.fake.set_latest_release(Some("v99.0.0"));

        h.daemon.check_release().await;

        let actions = &announced(&h)[0].actions;
        assert!(actions.iter().any(|a| a == "open-release"), "{actions:?}");
        assert!(!actions.iter().any(|a| a == START_UPDATE), "{actions:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn restart_does_not_repeat_offer() {
        let mut h = Harness::new().await;
        publish(&h, "v99.0.0", b"p", b"p");

        h.daemon.check_release().await;
        h.daemon.check_release().await;
        h.restart(None).await;
        h.daemon.check_release().await;

        assert_eq!(announced(&h).len(), 1, "{:?}", h.notes());
        publish(&h, "v99.0.1", b"p", b"p");
        h.daemon.check_release().await;
        assert_eq!(announced(&h).len(), 2, "a versão seguinte avisa logo");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn offer_repeats_after_a_day() {
        let h = Harness::new().await;
        offered(&h).await;
        let day_ago = gdl_common::time::now_ms() - 25 * 60 * 60 * 1000;
        let state = serde_json::json!({ "version": "99.0.0", "offered_ms": day_ago });
        write_saved(&h, state);

        h.daemon.check_release().await;

        assert_eq!(announced(&h).len(), 2);
    }

    /// A versão avisada pelo app antigo (`versao-avisada`) não avisa de novo antes de um dia.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn old_announced_file_is_migrated() {
        let h = Harness::new().await;
        std::fs::create_dir_all(&h.paths().state_dir).unwrap();
        std::fs::write(h.paths().state_dir.join("versao-avisada"), "99.0.0").unwrap();
        publish(&h, "v99.0.0", b"p", b"p");

        h.daemon.check_release().await;

        assert_eq!(
            announced(&h).len(),
            1,
            "o arquivo antigo não tem a hora: avisa"
        );
        assert!(!h.paths().state_dir.join("versao-avisada").exists());
        assert_eq!(saved(&h)["version"], "99.0.0");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn later_only_dismisses() {
        let h = Harness::new().await;
        offered(&h).await;

        h.daemon.run_action("", LATER).await.unwrap();

        assert!(h.installer.calls().is_empty());
        assert_eq!(h.daemon.update_state().1, UpdateStep::Available);
        assert!(h.daemon.update_state().0.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn update_installs_and_upgrades() {
        let h = Harness::new().await;
        publish(&h, "v99.0.0", b"pacote", b"pacote");
        h.daemon.check_release().await;

        h.daemon.run_action("", START_UPDATE).await.unwrap();

        wait_until("pedido de troca de programa", || {
            h.daemon.requested_exit() == Some(Exit::Upgrade)
        })
        .await;
        let calls = h.installer.calls();
        assert_eq!(calls.len(), 1, "{calls:?}");
        let (kind, package, sum) = &calls[0];
        assert_eq!(*kind, InstallKind::Deb);
        assert_eq!(
            package.file_name().unwrap().to_str(),
            Some(deb("v99.0.0").as_str())
        );
        assert_eq!(*sum, sha256_hex(b"pacote"));
        assert_eq!(saved(&h)["installing"], "99.0.0");
        assert!(!download_dir(&h).exists(), "baixado e apagado");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn updated_note_after_upgrade() {
        let h = Harness::new().await;
        let state = serde_json::json!({
            "version": text(installed()),
            "offered_ms": 0,
            "installing": text(installed()),
        });
        write_saved(&h, state);

        h.daemon.announce_upgrade().await;
        h.daemon.announce_upgrade().await;

        let notes = named(&h, UPDATED);
        assert_eq!(notes.len(), 1, "uma vez: {:?}", h.notes());
        assert!(
            notes[0].body.contains(&text(installed()))
                || notes[0].summary.contains(&text(installed()))
        );
        assert!(saved(&h)["installing"].is_null());
    }

    /// Partiu na versão antiga depois de pedir a nova: a instalação não pegou.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn upgrade_that_did_not_take_is_reported() {
        let h = Harness::new().await;
        let state =
            serde_json::json!({ "version": "99.0.0", "offered_ms": 0, "installing": "99.0.0" });
        write_saved(&h, state);

        h.daemon.announce_upgrade().await;

        assert_eq!(named(&h, FAILED).len(), 1, "{:?}", h.notes());
        assert!(named(&h, UPDATED).is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn bad_download_fails_safely() {
        let h = Harness::new().await;
        publish(&h, "v99.0.0", b"pacote", b"<html>erro</html>");
        h.daemon.check_release().await;

        h.daemon.update_now().await.unwrap();

        assert!(h.installer.calls().is_empty());
        assert_eq!(named(&h, FAILED).len(), 1, "{:?}", h.notes());
        assert!(!download_dir(&h).exists());
        assert_eq!(h.daemon.update_state().1, UpdateStep::Available);
        assert_eq!(h.daemon.requested_exit(), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn installer_failure_is_reported() {
        let h = Harness::new().await;
        offered(&h).await;
        *h.installer.result.lock().unwrap() =
            Err(crate::upgrade::UpgradeError::Install("saiu com 100".into()));

        h.daemon.update_now().await.unwrap();

        assert_eq!(named(&h, FAILED).len(), 1, "{:?}", h.notes());
        assert_eq!(h.daemon.update_state().1, UpdateStep::Available);
        assert_eq!(h.daemon.requested_exit(), None);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn canceled_password_is_quiet() {
        let h = Harness::new().await;
        offered(&h).await;
        *h.installer.result.lock().unwrap() = Ok(InstallOutcome::Canceled);

        h.daemon.update_now().await.unwrap();

        assert!(named(&h, FAILED).is_empty(), "{:?}", h.notes());
        assert_eq!(h.daemon.update_state().1, UpdateStep::Available);
        assert_eq!(h.daemon.requested_exit(), None);
        assert!(!download_dir(&h).exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn second_start_is_ignored() {
        let h = Harness::new().await;
        offered(&h).await;

        let (first, second) = tokio::join!(h.daemon.update_now(), h.daemon.update_now());

        assert!(first.is_ok() != second.is_ok(), "{first:?} {second:?}");
        assert_eq!(h.installer.calls().len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn source_install_opens_release_page() {
        let h = Harness::with_install_kind(InstallKind::Source).await;
        offered(&h).await;
        let actions = &announced(&h)[0].actions;
        assert!(!actions.iter().any(|a| a == START_UPDATE), "{actions:?}");

        h.daemon.update_now().await.unwrap();

        assert!(h.installer.calls().is_empty());
        wait_until("página da release", || {
            h.opened()
                .iter()
                .any(|o| o.contains("releases/tag/v99.0.0"))
        })
        .await;
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
