//! A consulta da última release e o aviso de nova versão (`crate::updates`). A versão avisada fica
//! em `<state_dir>/versao-avisada`: cada versão avisa uma vez, também depois de reiniciar.

use std::path::PathBuf;
use std::sync::Arc;

use gdl_common::fs::write_atomic;
use gdl_common::links::release_notes;
use gdl_common::paths::create_private_dir;

use super::{Daemon, ServiceError};
use crate::notify;
use crate::updates::{self, Version, installed, version};

impl Daemon {
    /// Consulta depois de `UpdateCheck::first` e a cada `every`, enquanto o serviço existir.
    pub fn watch_releases(self: &Arc<Self>) {
        let (first, every) = (self.config.updates.first, self.config.updates.every);
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep(first).await;
            while let Some(daemon) = daemon.upgrade() {
                daemon.check_release().await;
                // Sem segurar o serviço enquanto dorme: quando ele sai, o laço acaba.
                drop(daemon);
                tokio::time::sleep(every).await;
            }
        });
    }

    /// Uma consulta: versão maior que a instalada e ainda não avisada → aviso. Sem rede, sem
    /// release ou com resposta estranha, nada acontece (só o log detalhado registra).
    pub async fn check_release(&self) {
        let latest = match updates::latest(&self.config.updates.url).await {
            Ok(Some(latest)) => latest,
            Ok(None) => return,
            Err(e) => {
                tracing::debug!(erro = %e, "não deu para consultar a última versão");
                return;
            }
        };
        if latest <= installed() || self.announced_release() >= Some(latest) {
            return;
        }
        let text = updates::text(latest);
        // Grava antes de avisar: cada versão avisa no máximo uma vez, mesmo se o aviso falhar.
        let state_dir = &self.config.paths.state_dir;
        let saved = create_private_dir(state_dir)
            .and_then(|()| write_atomic(&self.announced_file(), text.as_bytes()));
        if let Err(e) = saved {
            tracing::warn!(erro = %e, "não deu para guardar a versão avisada");
        }
        self.notify(&notify::new_version(&text, &self.i18n)).await;
    }

    /// "Ver novidades": a página da versão avisada.
    pub(super) fn open_release(&self) -> Result<(), ServiceError> {
        let announced = self
            .announced_release()
            .ok_or_else(|| ServiceError::NotFound("versão avisada".into()))?;
        self.open(release_notes(&updates::text(announced)));
        Ok(())
    }

    fn announced_file(&self) -> PathBuf {
        self.config.paths.state_dir.join("versao-avisada")
    }

    fn announced_release(&self) -> Option<Version> {
        version(std::fs::read_to_string(self.announced_file()).ok()?.trim())
    }
}
