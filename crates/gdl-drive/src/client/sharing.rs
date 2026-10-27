//! Compartilhamento (`permissions`) e versões (`revisions`) de um item.

use bytes::Bytes;
use serde::Deserialize;
use serde_json::json;

use super::{DriveClient, Grantee, Permission, Result, Revision};

const PERMISSION_FIELDS: &str = "id,type,role,emailAddress,domain,displayName";
const REVISION_FIELDS: &str = "id,modifiedTime,size,keepForever,lastModifyingUser(displayName)";

impl DriveClient {
    /// `files/{file_id}/{rest}` na API.
    fn file_sub_api(&self, file_id: &str, rest: &str) -> url::Url {
        self.api(&format!("files/{file_id}/{rest}"))
    }

    /// Quem tem acesso ao item.
    pub async fn permissions(&self, file_id: &str) -> Result<Vec<Permission>> {
        #[derive(Deserialize)]
        struct Page {
            #[serde(default)]
            permissions: Vec<Permission>,
        }
        let url = self.file_sub_api(file_id, "permissions");
        let fields = format!("permissions({PERMISSION_FIELDS})");
        let page: Page = self
            .json(|| {
                self.http
                    .get(url.clone())
                    .query(&[("fields", fields.as_str()), ("pageSize", "100")])
            })
            .await?;
        Ok(page.permissions)
    }

    /// Dá acesso a `grantee` com o papel `role`. Para pessoas, `notify` manda o aviso por e-mail
    /// do Google, com a mensagem, se houver.
    pub async fn add_permission(
        &self,
        file_id: &str,
        grantee: &Grantee,
        role: &str,
        notify: Option<&str>,
    ) -> Result<Permission> {
        let body = match grantee {
            Grantee::Person(email) => {
                json!({ "type": "user", "role": role, "emailAddress": email })
            }
            Grantee::Domain(domain) => json!({ "type": "domain", "role": role, "domain": domain }),
            Grantee::Anyone => json!({ "type": "anyone", "role": role }),
        };
        let person = matches!(grantee, Grantee::Person(_));
        let mut query = vec![("fields", PERMISSION_FIELDS)];
        if person {
            query.push((
                "sendNotificationEmail",
                if notify.is_some() { "true" } else { "false" },
            ));
            if let Some(message) = notify.filter(|m| !m.is_empty()) {
                query.push(("emailMessage", message));
            }
        }
        let url = self.file_sub_api(file_id, "permissions");
        self.json(|| self.http.post(url.clone()).query(&query).json(&body))
            .await
    }

    pub async fn set_permission_role(
        &self,
        file_id: &str,
        permission_id: &str,
        role: &str,
    ) -> Result<Permission> {
        let url = self.file_sub_api(file_id, &format!("permissions/{permission_id}"));
        self.json(|| {
            self.http
                .patch(url.clone())
                .query(&[("fields", PERMISSION_FIELDS)])
                .json(&json!({ "role": role }))
        })
        .await
    }

    pub async fn remove_permission(&self, file_id: &str, permission_id: &str) -> Result<()> {
        let url = self.file_sub_api(file_id, &format!("permissions/{permission_id}"));
        self.call(|| self.http.delete(url.clone()), |_| async { Ok(()) })
            .await
    }

    /// As versões guardadas do conteúdo, da mais antiga para a atual.
    pub async fn revisions(&self, file_id: &str) -> Result<Vec<Revision>> {
        #[derive(Deserialize)]
        struct Page {
            #[serde(default)]
            revisions: Vec<Revision>,
        }
        let url = self.file_sub_api(file_id, "revisions");
        let fields = format!("revisions({REVISION_FIELDS})");
        let page: Page = self
            .json(|| {
                self.http
                    .get(url.clone())
                    .query(&[("fields", fields.as_str()), ("pageSize", "1000")])
            })
            .await?;
        Ok(page.revisions)
    }

    /// O conteúdo de uma versão.
    pub async fn download_revision(&self, file_id: &str, revision_id: &str) -> Result<Bytes> {
        let url = self.file_sub_api(file_id, &format!("revisions/{revision_id}"));
        self.call(
            || self.http.get(url.clone()).query(&[("alt", "media")]),
            |resp| async move { Ok(resp.bytes().await?) },
        )
        .await
    }

    /// Marca (ou desmarca) uma versão para o Drive nunca apagar.
    pub async fn set_keep_forever(
        &self,
        file_id: &str,
        revision_id: &str,
        keep: bool,
    ) -> Result<Revision> {
        let url = self.file_sub_api(file_id, &format!("revisions/{revision_id}"));
        self.json(|| {
            self.http
                .patch(url.clone())
                .query(&[("fields", REVISION_FIELDS)])
                .json(&json!({ "keepForever": keep }))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use gdl_fake_drive::FakeDrive;

    use super::super::tests::client;
    use super::*;

    #[tokio::test]
    async fn sharing_with_a_person_sends_the_message() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec());
        let c = client(&fake);

        let who = Grantee::Person("ana@example.com".into());
        let added = c
            .add_permission(&id, &who, "writer", Some("Olha isto"))
            .await
            .unwrap();

        assert_eq!(
            (added.kind.as_str(), added.role.as_str()),
            ("user", "writer")
        );
        let list = c.permissions(&id).await.unwrap();
        let roles: Vec<_> = list
            .iter()
            .map(|p| (p.email.clone(), p.role.as_str()))
            .collect();
        assert_eq!(
            roles,
            [
                (Some(fake.email()), "owner"),
                (Some("ana@example.com".into()), "writer")
            ]
        );
        assert_eq!(
            fake.share_emails(),
            [("ana@example.com".into(), Some("Olha isto".into()))]
        );
    }

    #[tokio::test]
    async fn sharing_without_notice_sends_no_email() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec());
        let who = Grantee::Person("ana@example.com".into());
        client(&fake)
            .add_permission(&id, &who, "reader", None)
            .await
            .unwrap();
        assert!(fake.share_emails().is_empty());
    }

    #[tokio::test]
    async fn general_access_changes_role_and_goes_away() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec());
        let c = client(&fake);

        let link = c
            .add_permission(&id, &Grantee::Anyone, "reader", None)
            .await
            .unwrap();
        c.set_permission_role(&id, &link.id, "commenter")
            .await
            .unwrap();
        let general = |list: Vec<Permission>| {
            list.into_iter()
                .filter(|p| p.kind == "anyone")
                .map(|p| p.role)
                .collect::<Vec<_>>()
        };
        assert_eq!(general(c.permissions(&id).await.unwrap()), ["commenter"]);

        c.remove_permission(&id, &link.id).await.unwrap();
        assert!(general(c.permissions(&id).await.unwrap()).is_empty());
    }

    #[tokio::test]
    async fn the_owner_cannot_be_removed() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec());
        assert!(client(&fake).remove_permission(&id, "owner").await.is_err());
        assert_eq!(fake.permissions(&id).len(), 1);
    }

    #[tokio::test]
    async fn read_only_items_cannot_be_shared() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec());
        fake.make_read_only(&id);
        let res = client(&fake)
            .add_permission(&id, &Grantee::Anyone, "reader", None)
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn revisions_keep_each_version_of_the_content() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"um".to_vec());
        fake.modify_remote(&id, b"dois".to_vec());
        fake.modify_remote(&id, b"tres!".to_vec());
        let c = client(&fake);

        let list = c.revisions(&id).await.unwrap();
        let sizes: Vec<_> = list.iter().map(|r| r.size).collect();
        assert_eq!(sizes, [Some(2), Some(4), Some(5)]);
        assert!(
            list.windows(2)
                .all(|w| w[0].modified_ms <= w[1].modified_ms)
        );
        let first = c.download_revision(&id, &list[0].id).await.unwrap();
        assert_eq!(first.as_ref(), b"um");

        let kept = c.set_keep_forever(&id, &list[0].id, true).await.unwrap();
        assert!(kept.keep_forever);
        assert!(c.revisions(&id).await.unwrap()[0].keep_forever);
    }

    #[tokio::test]
    async fn unknown_revision_is_an_error() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"um".to_vec());
        assert!(client(&fake).download_revision(&id, "nada").await.is_err());
    }
}
