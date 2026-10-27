//! Estado do Drive simulado: arquivos, histórico de mudanças, sessões de upload e tokens.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use md5::{Digest, Md5};
use rand::Rng;
use rand::distr::Alphanumeric;
use serde_json::{Value, json};
use sha2::Sha256;
use url::Url;

pub(crate) const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
pub(crate) const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";
const GOOGLE_APPS_PREFIX: &str = "application/vnd.google-apps.";

/// ID aleatório em base62, como os do Drive.
pub(crate) fn random_id(len: usize) -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub(crate) use gdl_common::time::{now_ms, parse_rfc3339, rfc3339};

fn md5_hex(bytes: &[u8]) -> String {
    Md5::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Estado compartilhado entre o servidor e o teste.
pub(crate) struct Shared {
    state: Mutex<State>,
    pub(crate) origin: Url,
    pub(crate) offline: AtomicBool,
    /// Bytes que ainda podem trafegar antes da queda; negativo = desligado.
    pub(crate) drop_budget: AtomicI64,
    /// Esgotado o limite, a conexão para de andar em vez de cair.
    pub(crate) stall: AtomicBool,
    /// O limite de bytes armado por último já se esgotou.
    pub(crate) limit_reached: AtomicBool,
    /// A próxima escrita (a resposta que acabou de ser montada) derruba a conexão antes de sair.
    pub(crate) cut_next_write: AtomicBool,
}

impl Shared {
    pub(crate) fn new(origin: Url) -> Shared {
        Shared {
            state: Mutex::new(State::new(origin.clone())),
            origin,
            offline: AtomicBool::new(false),
            drop_budget: AtomicI64::new(-1),
            stall: AtomicBool::new(false),
            limit_reached: AtomicBool::new(false),
            cut_next_write: AtomicBool::new(false),
        }
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Desconta `n` bytes do limite; devolve `true` (uma vez) quando ele se esgota.
    pub(crate) fn consume(&self, n: usize) -> bool {
        let n = n as i64;
        let reached = self
            .drop_budget
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| match left {
                l if l < 0 || n == 0 => None,
                l if l - n <= 0 => Some(-1),
                l => Some(l - n),
            })
            .is_ok_and(|left| left - n <= 0);
        if reached {
            self.limit_reached.store(true, Ordering::SeqCst);
        }
        reached
    }
}

pub(crate) struct FileRec {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) mime: String,
    pub(crate) parents: Vec<String>,
    pub(crate) content: Bytes,
    /// Calculado a cada versão nova do conteúdo, não a cada resposta.
    pub(crate) md5: String,
    pub(crate) head_revision: String,
    pub(crate) created_ms: i64,
    pub(crate) modified_ms: i64,
    pub(crate) trashed: bool,
    pub(crate) can_edit: bool,
    pub(crate) shortcut_target: Option<String>,
    /// Compartilhado com o usuário por outra pessoa (dono, data): fora do Meu Drive.
    pub(crate) shared: Option<(String, i64)>,
    /// Permissões no formato da API (`id`, `type`, `role`, `emailAddress`…).
    pub(crate) permissions: Vec<Value>,
    /// Versões anteriores do conteúdo, da mais antiga para a mais nova (a atual é o próprio item).
    pub(crate) old_revisions: Vec<OldRevision>,
    /// Revisões marcadas para "manter para sempre", inclusive a atual.
    pub(crate) keep_forever: HashSet<String>,
}

/// O que uma listagem ou lista de mudanças abrange (`corpora` e `includeItemsFromAllDrives`).
pub(crate) enum Scope {
    /// Só o que não é de Drive compartilhado.
    MyDrive,
    /// Só o Drive compartilhado com esse ID.
    Drive(String),
    /// Tudo o que a conta vê, inclusive Drives compartilhados.
    Everything,
}

/// Um Drive compartilhado: a raiz dele é a pasta de mesmo ID.
pub(crate) struct DriveRec {
    pub(crate) id: String,
    pub(crate) name: String,
    /// Falso para quem só vê: nada novo cabe dentro (`drives.get` → `capabilities`).
    pub(crate) can_add_children: bool,
    /// Contas que veem tudo dentro dele.
    pub(crate) members: Vec<String>,
    /// O `requestId` da criação, para repeti-la sem duplicar.
    request_id: Option<String>,
}

pub(crate) struct OldRevision {
    pub(crate) id: String,
    pub(crate) content: Bytes,
    pub(crate) modified_ms: i64,
}

impl FileRec {
    /// Pastas, atalhos e Documentos Google não têm bytes para baixar.
    pub(crate) fn has_content(&self) -> bool {
        !self.mime.starts_with(GOOGLE_APPS_PREFIX)
    }
}

/// Erro HTTP injetado nas próximas `times` chamadas à API.
pub(crate) struct StatusFault {
    pub(crate) code: u16,
    pub(crate) reason: &'static str,
    pub(crate) times: u32,
}

/// Sessão de upload retomável. Como no Drive real, cada requisição nova substitui a anterior: uma
/// conexão morta (ou empacada) não segura a sessão.
pub(crate) struct Session {
    /// Arquivo a atualizar; `None` cria um novo.
    pub(crate) target: Option<String>,
    /// Quem abriu a sessão (a URL dela não leva o token).
    pub(crate) account: String,
    pub(crate) meta: Value,
    pub(crate) data: Vec<u8>,
    /// Requisição dona da sessão agora; a substituída para de gravar.
    pub(crate) owner: u64,
}

pub(crate) type SessionRef = Arc<tokio::sync::Mutex<Session>>;

pub(crate) struct State {
    /// Origem do servidor, para montar os endereços absolutos (miniaturas).
    origin: Url,
    /// Destino do 302 que a rota de miniaturas devolve, quando definido.
    pub(crate) thumb_redirect: Option<String>,
    /// Atraso antes de responder cada miniatura (`Fault::DelayThumbs`).
    pub(crate) thumb_delay: Option<std::time::Duration>,
    /// Atraso antes de responder cada envio de conteúdo (`Fault::DelayUploads`).
    pub(crate) upload_delay: Option<std::time::Duration>,
    /// Envios de conteúdo em andamento agora e o máximo já visto.
    pub(crate) uploads_in_flight: u32,
    pub(crate) max_uploads_in_flight: u32,
    /// Erro de cada miniatura (`Fault::ThumbStatus`).
    pub(crate) thumb_status: Option<u16>,
    /// Corpo de cada miniatura cortado no meio (`Fault::CutThumbBody`).
    pub(crate) thumb_cut: bool,
    pub(crate) files: BTreeMap<String, FileRec>,
    pub(crate) drives: Vec<DriveRec>,
    /// A conta principal (a dos auxiliares do `FakeDrive` e do `TEST_ACCESS_TOKEN`).
    pub(crate) root_id: String,
    pub(crate) email: String,
    /// Todas as contas, com a raiz do Meu Drive de cada uma (inclusive a principal).
    pub(crate) accounts: BTreeMap<String, String>,
    /// Conta do próximo login (`FakeDrive::login_as`); sem ela, a principal.
    pub(crate) next_login: Option<String>,
    clock_ms: i64,
    change_seq: u64,
    changes: Vec<(u64, String)>,
    pub(crate) sessions: HashMap<String, SessionRef>,
    /// Código de autorização → (desafio S256, conta).
    codes: HashMap<String, (String, String)>,
    /// Token de acesso → (validade, conta).
    access_tokens: HashMap<String, (i64, String)>,
    /// Refresh token → conta.
    refresh_tokens: HashMap<String, String>,
    pub(crate) token_lifetime_s: i64,
    pub(crate) fault: Option<StatusFault>,
    /// Avisos por e-mail de compartilhamento: (e-mail, mensagem).
    pub(crate) share_emails: Vec<(String, Option<String>)>,
    pub(crate) request_log: Vec<String>,
    /// Método HTTP de cada requisição, na mesma ordem de `request_log`.
    pub(crate) method_log: Vec<String>,
    pub(crate) downloaded: u64,
    /// Edição a aplicar depois de servir tantas páginas de `files.list`: (páginas, ID, conteúdo).
    pub(crate) edit_during_listing: Option<(u32, String, Vec<u8>)>,
    /// IDs entregues por `files.generateIds` e ainda não usados.
    pub(crate) generated_ids: HashSet<String>,
    /// Quantas das próximas respostas de criação se perdem depois de processado o pedido.
    pub(crate) lose_responses: u32,
}

/// Por que um `id` pedido na criação não serve.
pub(crate) enum IdRefused {
    /// Já existe um item com ele: a criação anterior deu certo.
    Duplicate,
    /// Não veio de `files.generateIds`.
    NotUsable,
}

impl State {
    fn new(origin: Url) -> State {
        let mut state = State {
            origin,
            thumb_redirect: None,
            thumb_delay: None,
            upload_delay: None,
            uploads_in_flight: 0,
            max_uploads_in_flight: 0,
            thumb_status: None,
            thumb_cut: false,
            files: BTreeMap::new(),
            drives: vec![],
            root_id: String::new(),
            email: "teste@example.com".into(),
            accounts: BTreeMap::new(),
            next_login: None,
            clock_ms: 0,
            change_seq: 0,
            changes: vec![],
            sessions: HashMap::new(),
            codes: HashMap::new(),
            access_tokens: HashMap::new(),
            refresh_tokens: HashMap::new(),
            token_lifetime_s: 3599,
            fault: None,
            share_emails: vec![],
            request_log: vec![],
            method_log: vec![],
            downloaded: 0,
            edit_during_listing: None,
            generated_ids: HashSet::new(),
            lose_responses: 0,
        };
        let email = state.email.clone();
        state.root_id = state.add_account(&email);
        state
    }

    /// Cria a conta `email` com o Meu Drive vazio e devolve a raiz dele.
    pub(crate) fn add_account(&mut self, email: &str) -> String {
        let root = random_id(28);
        self.accounts.insert(email.into(), root.clone());
        self.insert(root.clone(), "", "My Drive", FOLDER_MIME, Vec::new(), email);
        self.file_mut(&root).parents.clear();
        root
    }

    /// Cria um Drive compartilhado com `member` dentro, e devolve o ID (o da pasta raiz). Com o
    /// mesmo `request_id` de uma criação anterior, devolve o que já existe.
    pub(crate) fn add_drive(
        &mut self,
        name: &str,
        member: &str,
        request_id: Option<&str>,
    ) -> String {
        if let Some(rid) = request_id
            && let Some(drive) = self
                .drives
                .iter()
                .find(|d| d.request_id.as_deref() == Some(rid))
        {
            return drive.id.clone();
        }
        let id = random_id(19);
        self.insert(id.clone(), "", name, FOLDER_MIME, Vec::new(), member);
        self.file_mut(&id).parents.clear();
        self.drives.push(DriveRec {
            id: id.clone(),
            name: name.into(),
            can_add_children: true,
            members: vec![member.into()],
            request_id: request_id.map(String::from),
        });
        id
    }

    /// Dá o Drive compartilhado `id` a `email`.
    pub(crate) fn add_member(&mut self, id: &str, email: &str) {
        let drive = self
            .drives
            .iter_mut()
            .find(|d| d.id == id)
            .unwrap_or_else(|| panic!("Drive compartilhado {id} não existe no Drive simulado"));
        drive.members.push(email.into());
    }

    /// O Drive compartilhado onde o item está: o primeiro item acima dele (ou ele mesmo) cujo ID
    /// é o de um drive.
    pub(crate) fn drive_of(&self, f: &FileRec) -> Option<&str> {
        let mut current = Some(f);
        while let Some(file) = current {
            if let Some(drive) = self.drives.iter().find(|d| d.id == file.id) {
                return Some(&drive.id);
            }
            current = file.parents.first().and_then(|p| self.files.get(p));
        }
        None
    }

    /// Os Drives compartilhados de que `account` é membro.
    pub(crate) fn drives_of<'a>(&'a self, account: &'a str) -> impl Iterator<Item = &'a DriveRec> {
        self.drives
            .iter()
            .filter(move |d| d.members.iter().any(|m| m == account))
    }

    pub(crate) fn root_of(&self, account: &str) -> &str {
        self.accounts
            .get(account)
            .map_or(&self.root_id, String::as_str)
    }

    /// O dono de um item novo em `parent`: o do pai (a principal, sem pai).
    fn owner_under(&self, parent: &str) -> String {
        self.files
            .get(parent)
            .and_then(|f| f.permissions.iter().find(|p| p["role"] == "owner"))
            .and_then(|p| p["emailAddress"].as_str())
            .unwrap_or(&self.email)
            .to_string()
    }

    /// `account` vê o item: está no Meu Drive dela, num Drive compartilhado de que é membro ou
    /// foi compartilhado com ela (ele ou uma pasta acima). Com uma conta só, tudo o que não é de
    /// Drive compartilhado.
    pub(crate) fn visible_to(&self, account: &str, f: &FileRec) -> bool {
        if let Some(drive) = self.drive_of(f) {
            return self.drives_of(account).any(|d| d.id == drive);
        }
        if self.accounts.len() == 1 {
            return true;
        }
        let root = self.root_of(account);
        let mut current = Some(f);
        while let Some(file) = current {
            let mine = file.id == root
                || file
                    .permissions
                    .iter()
                    .any(|p| p["emailAddress"].as_str() == Some(account));
            if mine {
                return true;
            }
            current = file.parents.first().and_then(|p| self.files.get(p));
        }
        false
    }

    /// Relógio que sempre avança, para a ordem de criação ser estável.
    fn tick(&mut self) -> i64 {
        self.clock_ms = (self.clock_ms + 1).max(now_ms());
        self.clock_ms
    }

    pub(crate) fn resolve<'a>(&'a self, id: &'a str) -> &'a str {
        self.resolve_for(&self.email, id)
    }

    /// `id`, com o apelido `root` trocado pela raiz do Meu Drive de `account`.
    pub(crate) fn resolve_for<'a>(&'a self, account: &str, id: &'a str) -> &'a str {
        if id == "root" {
            self.root_of(account)
        } else {
            id
        }
    }

    pub(crate) fn file_mut(&mut self, id: &str) -> &mut FileRec {
        self.files
            .get_mut(id)
            .unwrap_or_else(|| panic!("arquivo {id} não existe no Drive simulado"))
    }

    pub(crate) fn record_change(&mut self, id: &str) {
        self.change_seq += 1;
        self.changes.push((self.change_seq, id.to_string()));
    }

    pub(crate) fn add(
        &mut self,
        parent: &str,
        name: &str,
        mime: &str,
        content: Vec<u8>,
        target: Option<String>,
    ) -> String {
        let owner = self.owner_under(parent);
        let id = self.insert(random_id(28), parent, name, mime, content, &owner);
        self.file_mut(&id).shortcut_target = target;
        id
    }

    fn insert(
        &mut self,
        id: String,
        parent: &str,
        name: &str,
        mime: &str,
        content: Vec<u8>,
        owner: &str,
    ) -> String {
        let now = self.tick();
        let md5 = md5_hex(&content);
        let owner = json!({
            "id": "owner", "type": "user", "role": "owner",
            "emailAddress": owner, "displayName": "Usuário de teste",
        });
        self.files.insert(
            id.clone(),
            FileRec {
                id: id.clone(),
                name: name.into(),
                mime: mime.into(),
                parents: vec![parent.to_string()],
                content: content.into(),
                md5,
                head_revision: random_id(20),
                created_ms: now,
                modified_ms: now,
                trashed: false,
                can_edit: true,
                shortcut_target: None,
                shared: None,
                permissions: vec![owner],
                old_revisions: vec![],
                keep_forever: HashSet::new(),
            },
        );
        self.record_change(&id);
        id
    }

    /// Item de outra pessoa compartilhado com o usuário. Como no Drive real, ele não está em
    /// nenhuma pasta do Meu Drive (sem pai) nem nas mudanças, mas a listagem traz o item e o que
    /// houver dentro dele.
    pub(crate) fn add_shared(
        &mut self,
        name: &str,
        mime: &str,
        owner: &str,
        content: Vec<u8>,
    ) -> String {
        let id = self.add("", name, mime, content, None);
        self.changes.pop();
        let shared_ms = self.tick();
        let email = self.email.clone();
        let file = self.file_mut(&id);
        file.parents.clear();
        file.shared = Some((owner.into(), shared_ms));
        file.permissions = vec![
            json!({ "id": "owner", "type": "user", "role": "owner", "displayName": owner }),
            json!({ "id": "me", "type": "user", "role": "writer", "emailAddress": email }),
        ];
        id
    }

    /// Troca o conteúdo: nova revisão, data de modificação e uma mudança no histórico.
    pub(crate) fn set_content(&mut self, id: &str, content: Vec<u8>, modified_ms: Option<i64>) {
        let now = self.tick();
        let file = self.file_mut(id);
        let old = OldRevision {
            id: std::mem::replace(&mut file.head_revision, random_id(20)),
            content: std::mem::replace(&mut file.content, content.into()),
            modified_ms: file.modified_ms,
        };
        file.old_revisions.push(old);
        file.md5 = md5_hex(&file.content);
        file.modified_ms = modified_ms.unwrap_or(now);
        self.record_change(id);
    }

    /// Aplica os metadados aceitos em `files.create`/`files.update`.
    pub(crate) fn apply_meta(&mut self, id: &str, meta: &Value) {
        let file = self.file_mut(id);
        if let Some(name) = meta["name"].as_str() {
            file.name = name.into();
        }
        if let Some(trashed) = meta["trashed"].as_bool() {
            file.trashed = trashed;
        }
        if let Some(ms) = meta["modifiedTime"].as_str().and_then(parse_rfc3339) {
            file.modified_ms = ms;
        }
        self.record_change(id);
    }

    /// Confere o `id` pedido na criação, se houver; como no Drive, só servem os gerados.
    pub(crate) fn check_requested_id(&self, meta: &Value) -> Result<Option<String>, IdRefused> {
        let Some(id) = meta["id"].as_str() else {
            return Ok(None);
        };
        if self.files.contains_key(id) {
            Err(IdRefused::Duplicate)
        } else if !self.generated_ids.contains(id) {
            Err(IdRefused::NotUsable)
        } else {
            Ok(Some(id.into()))
        }
    }

    pub(crate) fn generate_ids(&mut self, count: usize) -> Vec<String> {
        let ids: Vec<String> = (0..count).map(|_| random_id(28)).collect();
        self.generated_ids.extend(ids.iter().cloned());
        ids
    }

    /// Cria um arquivo de `account` a partir dos metadados de `files.create` ou de um upload; sem
    /// `parents`, no Meu Drive dela.
    pub(crate) fn create(
        &mut self,
        account: &str,
        meta: &Value,
        content: Vec<u8>,
    ) -> Result<String, IdRefused> {
        let id = match self.check_requested_id(meta)? {
            Some(id) => {
                self.generated_ids.remove(&id);
                id
            }
            None => random_id(28),
        };
        let parent = meta["parents"][0]
            .as_str()
            .unwrap_or(self.root_of(account))
            .to_string();
        let mime = meta["mimeType"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_string();
        let target = meta["shortcutDetails"]["targetId"]
            .as_str()
            .map(String::from);
        let name = meta["name"].as_str().unwrap_or("Sem título").to_string();
        let id = self.insert(id, &parent, &name, &mime, content, account);
        self.file_mut(&id).shortcut_target = target;
        self.apply_meta(&id, meta);
        Ok(id)
    }

    pub(crate) fn to_json(&self, f: &FileRec) -> Value {
        let mut v = json!({
            "kind": "drive#file",
            "id": f.id,
            "name": f.name,
            "mimeType": f.mime,
            "parents": f.parents,
            "trashed": f.trashed,
            "createdTime": rfc3339(f.created_ms),
            "modifiedTime": rfc3339(f.modified_ms),
            "capabilities": { "canEdit": f.can_edit },
            "webViewLink": format!("https://drive.google.com/open?id={}", f.id),
        });
        if f.has_content() {
            v["size"] = f.content.len().to_string().into();
            v["md5Checksum"] = f.md5.clone().into();
            v["headRevisionId"] = f.head_revision.clone().into();
        }
        if f.mime.starts_with("image/") && !f.content.is_empty() {
            let link = self.origin.join(&format!("thumb/{}=s220", f.id));
            v["thumbnailLink"] = link.expect("caminho relativo válido").to_string().into();
        }
        if let Some(drive) = self.drive_of(f) {
            v["driveId"] = drive.into();
        }
        if let Some((owner, shared_ms)) = &f.shared {
            v["sharedWithMeTime"] = rfc3339(*shared_ms).into();
            v["owners"] = json!([{ "displayName": owner }]);
        }
        if let Some(target) = &f.shortcut_target {
            let target_mime = self
                .files
                .get(target)
                .map_or("application/octet-stream", |t| t.mime.as_str());
            v["shortcutDetails"] = json!({ "targetId": target, "targetMimeType": target_mime });
        }
        v
    }

    /// Metadados de `id` (aceita o apelido `root`), ou `None` se não existir.
    pub(crate) fn file_json(&self, account: &str, id: &str) -> Option<Value> {
        self.files
            .get(self.resolve_for(account, id))
            .map(|f| self.to_json(f))
    }

    pub(crate) fn start_page_token(&self) -> String {
        (self.change_seq + 1).to_string()
    }

    /// O item entra na listagem de `account` no escopo `scope`. A raiz do Drive não é um
    /// arquivo da listagem.
    pub(crate) fn listed_for(&self, account: &str, scope: &Scope, f: &FileRec) -> bool {
        let drive = self.drive_of(f);
        let in_scope = match scope {
            Scope::MyDrive => drive.is_none(),
            Scope::Drive(id) => drive == Some(id.as_str()),
            Scope::Everything => true,
        };
        in_scope && drive != Some(f.id.as_str()) && self.visible_to(account, f)
    }

    /// Mudanças a partir de `token`, uma por arquivo (o estado atual), em páginas.
    pub(crate) fn changes_since(
        &self,
        account: &str,
        scope: &Scope,
        token: u64,
        page_size: usize,
    ) -> Value {
        let mut seen = HashSet::new();
        let pending: Vec<_> = self
            .changes
            .iter()
            .filter(|(seq, _)| *seq >= token)
            .filter(|(_, id)| seen.insert(id.as_str()))
            .filter(|(_, id)| {
                self.files
                    .get(id)
                    .is_none_or(|f| self.listed_for(account, scope, f))
            })
            .collect();
        let page = &pending[..pending.len().min(page_size)];
        let changes: Vec<Value> = page
            .iter()
            .map(|(_, id)| match self.files.get(id) {
                Some(f) => json!({ "changeType": "file", "fileId": id, "removed": false, "file": self.to_json(f) }),
                None => json!({ "changeType": "file", "fileId": id, "removed": true }),
            })
            .collect();
        match pending.get(page_size) {
            Some((seq, _)) => json!({ "changes": changes, "nextPageToken": seq.to_string() }),
            None => json!({ "changes": changes, "newStartPageToken": self.start_page_token() }),
        }
    }

    /// Código para a conta do próximo login (`next_login`, que vale uma vez) ou a principal.
    pub(crate) fn issue_code(&mut self, challenge: &str) -> String {
        let code = random_id(32);
        let account = self.next_login.take().unwrap_or_else(|| self.email.clone());
        self.codes.insert(code.clone(), (challenge.into(), account));
        code
    }

    /// Troca o código por tokens se o `code_verifier` bater com o desafio S256.
    pub(crate) fn exchange_code(&mut self, code: &str, verifier: &str) -> Option<Value> {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if self.codes.get(code).is_none_or(|(c, _)| *c != challenge) {
            return None;
        }
        let (_, account) = self.codes.remove(code)?;
        let refresh = random_id(40);
        self.refresh_tokens.insert(refresh.clone(), account.clone());
        let mut tokens = self.new_access_token(&account);
        tokens["refresh_token"] = refresh.into();
        Some(tokens)
    }

    /// Esquece os tokens da conta: o acesso vira 401 e a renovação, `invalid_grant`.
    pub(crate) fn revoke(&mut self, account: &str) {
        self.access_tokens.retain(|_, (_, owner)| owner != account);
        self.refresh_tokens.retain(|_, owner| owner != account);
    }

    pub(crate) fn refresh(&mut self, refresh_token: &str) -> Option<Value> {
        let account = self.refresh_tokens.get(refresh_token)?.clone();
        Some(self.new_access_token(&account))
    }

    fn new_access_token(&mut self, account: &str) -> Value {
        let token = self.mint_token(account);
        json!({
            "access_token": token,
            "expires_in": self.token_lifetime_s,
            "scope": "https://www.googleapis.com/auth/drive",
            "token_type": "Bearer",
        })
    }

    /// A conta do token de acesso, se ele vale.
    pub(crate) fn account_of(&self, token: &str) -> Option<String> {
        if token == crate::TEST_ACCESS_TOKEN {
            return Some(self.email.clone());
        }
        self.access_tokens
            .get(token)
            .filter(|(exp, _)| *exp > now_ms())
            .map(|(_, account)| account.clone())
    }

    /// Um token de acesso válido para `account`, sem passar pelo login.
    pub(crate) fn mint_token(&mut self, account: &str) -> String {
        let token = random_id(40);
        let expires = now_ms() + self.token_lifetime_s * 1000;
        self.access_tokens
            .insert(token.clone(), (expires, account.into()));
        token
    }
}

/// Uma cláusula do `q` do `files.list` (só as que o cliente usa).
pub(crate) enum Clause {
    InParents(String),
    Always,
    Trashed(bool),
    NameContains(String),
    NameIs(String),
    FullText(String),
    MimeIs(String),
    MimeIsNot(String),
    SharedWithMe,
}

impl Clause {
    pub(crate) fn matches(&self, f: &FileRec) -> bool {
        match self {
            Clause::InParents(id) => f.parents.contains(id),
            Clause::Always => true,
            Clause::Trashed(t) => f.trashed == *t,
            Clause::NameContains(t) => f.name.contains(t.as_str()),
            Clause::NameIs(t) => f.name == *t,
            Clause::FullText(t) => {
                f.name.contains(t.as_str())
                    || String::from_utf8_lossy(&f.content).contains(t.as_str())
            }
            Clause::MimeIs(t) => f.mime == *t,
            Clause::MimeIsNot(t) => f.mime != *t,
            Clause::SharedWithMe => f.shared.is_some(),
        }
    }
}

/// Interpreta um `q` com cláusulas unidas por `and`.
pub(crate) fn parse_query(q: &str) -> Result<Vec<Clause>, String> {
    q.split(" and ")
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|clause| {
            Ok(if let Some(rest) = clause.strip_suffix(" in parents") {
                Clause::InParents(literal(rest)?)
            } else if clause.ends_with(" in owners") {
                Clause::Always
            } else if clause == "sharedWithMe = true" {
                Clause::SharedWithMe
            } else if let Some(rest) = clause.strip_prefix("trashed =") {
                Clause::Trashed(rest.trim() == "true")
            } else if let Some(rest) = clause.strip_prefix("name contains") {
                Clause::NameContains(literal(rest)?)
            } else if let Some(rest) = clause.strip_prefix("name =") {
                Clause::NameIs(literal(rest)?)
            } else if let Some(rest) = clause.strip_prefix("fullText contains") {
                Clause::FullText(literal(rest)?)
            } else if let Some(rest) = clause.strip_prefix("mimeType !=") {
                Clause::MimeIsNot(literal(rest)?)
            } else if let Some(rest) = clause.strip_prefix("mimeType =") {
                Clause::MimeIs(literal(rest)?)
            } else {
                return Err(format!("cláusula não suportada: {clause}"));
            })
        })
        .collect()
}

/// Texto entre aspas simples, com `\'` e `\\` desfeitos.
fn literal(text: &str) -> Result<String, String> {
    let inner = text
        .trim()
        .strip_prefix('\'')
        .and_then(|t| t.strip_suffix('\''))
        .ok_or_else(|| format!("literal inválido: {text}"))?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' {
            chars.next().unwrap_or('\\')
        } else {
            c
        });
    }
    Ok(out)
}
