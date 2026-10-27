//! As janelas "Compartilhar" e "Gerenciar versões": o serviço só repassa ao motor, conferindo
//! os argumentos e montando as respostas.

use gdl_common::names::{create_unique, split_ext};
use gdl_common::paths::user_dir;
use gdl_common::time::local_stamp;
use gdl_drive::model::{Grantee, Permission};
use gdl_engine::engine::Engine;
use gdl_ipc::{ServiceError, SharePerson, Sharing, VersionItem, Versions};
use gdl_store::ItemId;

use super::{Daemon, engine_error, failed, live_item, web_link};
use crate::notify::Notification;

/// Os papéis que a janela pode dar (dono não se dá por aqui).
const ROLES: [&str; 3] = ["reader", "commenter", "writer"];

/// O acesso geral do item; com os dois, vale o de qualquer pessoa (o mais amplo).
fn general_access(perms: &[Permission]) -> Option<&Permission> {
    perms
        .iter()
        .filter(|p| p.is_general())
        .max_by_key(|p| p.kind == "anyone")
}

/// O domínio de uma conta do Google Workspace; vazio em contas pessoais.
fn workspace_domain(email: &str) -> String {
    match email.rsplit_once('@') {
        Some((_, "gmail.com" | "googlemail.com")) | None => String::new(),
        Some((_, domain)) => domain.to_lowercase(),
    }
}

fn check_role(role: &str) -> Result<(), ServiceError> {
    if !ROLES.contains(&role) {
        return Err(ServiceError::InvalidArgs(format!(
            "papel desconhecido: {role}"
        )));
    }
    Ok(())
}

impl Daemon {
    async fn account_domain(&self) -> String {
        self.with_account(|a| workspace_domain(&a.email))
            .await
            .unwrap_or_default()
    }

    pub async fn sharing(&self, item_id: &str) -> Result<Sharing, ServiceError> {
        let engine = self.connected_engine().await?;
        let item = live_item(&engine, item_id)?;
        let perms = engine
            .permissions(&item.id)
            .await
            .map_err(|e| engine_error(e, item_id))?;
        let general = general_access(&perms);
        let mut people: Vec<SharePerson> = perms
            .iter()
            .filter(|p| !p.is_general())
            .map(|p| SharePerson {
                id: p.id.clone(),
                email: p.email.clone().unwrap_or_default(),
                name: p.display_name.clone().unwrap_or_default(),
                role: p.role.clone(),
            })
            .collect();
        // O dono primeiro, como na janela original.
        people.sort_by_key(|p| p.role != "owner");
        Ok(Sharing {
            name: item.local_name.clone(),
            link: web_link(&item)?,
            can_share: item.can_edit,
            general: general.map_or("restricted", |p| p.kind.as_str()).into(),
            general_role: general.map(|p| p.role.clone()).unwrap_or_default(),
            domain: self.account_domain().await,
            people,
        })
    }

    /// O aviso de "link copiado", dizendo quem pode abrir o link. Sem resposta do Drive, nenhum.
    pub(super) async fn notify_link_copied(&self, engine: &Engine, id: &ItemId) {
        let Ok(perms) = engine.permissions(id).await else {
            return;
        };
        let body = match general_access(&perms) {
            None => self.i18n.tr("notify-link-restricted", &[]),
            Some(p) => {
                let role = self.i18n.tr(&format!("link-role-{}", p.role), &[]);
                let domain = p.domain.clone().unwrap_or_default();
                let key = format!("notify-link-{}", p.kind);
                self.i18n.tr(&key, &[("role", &role), ("domain", &domain)])
            }
        };
        self.notify(&Notification {
            kind_id: "link-copied",
            summary: self.i18n.tr("notify-link-copied-title", &[]),
            body,
            action: None,
            replaces_key: Some("link-copied".into()),
        })
        .await;
    }

    pub async fn share_add(
        &self,
        item_id: &str,
        emails: &[String],
        role: &str,
        notify: bool,
        message: &str,
    ) -> Result<(), ServiceError> {
        check_role(role)?;
        let emails: Vec<&str> = emails
            .iter()
            .map(|e| e.trim())
            .filter(|e| !e.is_empty())
            .collect();
        if emails.is_empty() || emails.iter().any(|e| !e.contains('@')) {
            return Err(ServiceError::InvalidArgs("e-mail inválido".into()));
        }
        let engine = self.connected_engine().await?;
        let id = ItemId(item_id.into());
        for email in emails {
            let who = Grantee::Person(email.into());
            engine
                .share(&id, &who, role, notify.then_some(message))
                .await
                .map_err(|e| engine_error(e, item_id))?;
        }
        Ok(())
    }

    pub async fn share_set_role(
        &self,
        item_id: &str,
        permission_id: &str,
        role: &str,
    ) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        let id = ItemId(item_id.into());
        let done = match role {
            "" => engine.unshare(&id, permission_id).await,
            role => {
                check_role(role)?;
                engine.set_share_role(&id, permission_id, role).await
            }
        };
        done.map_err(|e| engine_error(e, item_id))
    }

    /// Troca o acesso geral do item (`restricted`, `anyone` ou `domain`).
    pub async fn share_set_general(
        &self,
        item_id: &str,
        kind: &str,
        role: &str,
    ) -> Result<(), ServiceError> {
        let grantee = match kind {
            "restricted" => None,
            "anyone" => Some(Grantee::Anyone),
            "domain" => match self.account_domain().await {
                d if d.is_empty() => {
                    return Err(ServiceError::InvalidArgs("conta sem domínio".into()));
                }
                d => Some(Grantee::Domain(d)),
            },
            other => {
                return Err(ServiceError::InvalidArgs(format!(
                    "acesso desconhecido: {other}"
                )));
            }
        };
        if grantee.is_some() {
            check_role(role)?;
        }
        let engine = self.connected_engine().await?;
        engine
            .set_general_access(&ItemId(item_id.into()), grantee.as_ref(), role)
            .await
            .map_err(|e| engine_error(e, item_id))
    }

    pub async fn versions(&self, item_id: &str) -> Result<Versions, ServiceError> {
        let engine = self.connected_engine().await?;
        let item = live_item(&engine, item_id)?;
        let list = engine
            .revisions(&item.id)
            .await
            .map_err(|e| engine_error(e, item_id))?;
        Ok(Versions {
            name: item.local_name,
            can_edit: item.can_edit,
            versions: list
                .into_iter()
                .enumerate()
                .map(|(n, r)| VersionItem {
                    id: r.id,
                    modified_ms: r.modified_ms,
                    size: r.size.map_or(-1, |s| s as i64),
                    keep_forever: r.keep_forever,
                    author: r.author.unwrap_or_default(),
                    current: n == 0,
                })
                .collect(),
        })
    }

    /// Salva a versão na pasta de downloads como `nome (data hora).ext`, sem sobrescrever nada.
    pub async fn version_download(
        &self,
        item_id: &str,
        version_id: &str,
    ) -> Result<String, ServiceError> {
        let engine = self.connected_engine().await?;
        let item = live_item(&engine, item_id)?;
        let fail = |e| engine_error(e, item_id);
        // A data da versão (para o nome) e o conteúdo, pedidos juntos.
        let (list, content) = tokio::try_join!(
            engine.revisions(&item.id),
            engine.download_revision(&item.id, version_id)
        )
        .map_err(fail)?;
        let version = list
            .iter()
            .find(|r| r.id == version_id)
            .ok_or_else(|| ServiceError::NotFound(version_id.into()))?;
        let (stem, ext) = split_ext(&item.local_name);
        let stem = format!("{stem} ({})", local_stamp(version.modified_ms));
        let ext = ext.to_string();
        let (config_home, home) = (
            self.config.paths.config_home.clone(),
            self.config.home.clone(),
        );
        let saved = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
            let dir = user_dir(&config_home, &home, "DOWNLOAD", "Downloads");
            std::fs::create_dir_all(&dir)?;
            let (path, mut file) = create_unique(&dir, &stem, |n| match n {
                0 => ext.clone(),
                n => format!(" ({n}){ext}"),
            })?;
            std::io::Write::write_all(&mut file, &content)?;
            Ok(path.to_string_lossy().into_owned())
        });
        saved.await.map_err(failed)?.map_err(failed)
    }

    pub async fn version_restore(
        &self,
        item_id: &str,
        version_id: &str,
    ) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        engine
            .restore_revision(&ItemId(item_id.into()), version_id)
            .await
            .map_err(|e| engine_error(e, item_id))
    }

    pub async fn version_keep(
        &self,
        item_id: &str,
        version_id: &str,
        keep: bool,
    ) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        engine
            .keep_revision(&ItemId(item_id.into()), version_id, keep)
            .await
            .map_err(|e| engine_error(e, item_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_workspace_accounts_have_a_domain() {
        assert_eq!(workspace_domain("ana@gmail.com"), "");
        assert_eq!(workspace_domain("ana@googlemail.com"), "");
        assert_eq!(workspace_domain("ana@Empresa.com.br"), "empresa.com.br");
        assert_eq!(workspace_domain("sem-arroba"), "");
    }
}
