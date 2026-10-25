//! Estado do Drive simulado: arquivos, histórico de mudanças, sessões de upload e tokens.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, SecondsFormat};
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

pub(crate) fn now_ms() -> i64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    since_epoch.as_millis() as i64
}

pub(crate) fn rfc3339(ms: i64) -> String {
    DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn parse_rfc3339(text: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|d| d.timestamp_millis())
}

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
}

impl Shared {
    pub(crate) fn new(origin: Url) -> Shared {
        Shared {
            state: Mutex::new(State::new()),
            origin,
            offline: AtomicBool::new(false),
            drop_budget: AtomicI64::new(-1),
        }
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Desconta `n` bytes do limite; devolve `true` (uma vez) quando ele se esgota.
    pub(crate) fn consume(&self, n: usize) -> bool {
        let n = n as i64;
        self.drop_budget
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| match left {
                l if l < 0 || n == 0 => None,
                l if l - n <= 0 => Some(-1),
                l => Some(l - n),
            })
            .is_ok_and(|left| left - n <= 0)
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

/// Sessão de upload retomável. O lock próprio faz a consulta de offset esperar o envio em andamento.
pub(crate) struct Session {
    /// Arquivo a atualizar; `None` cria um novo.
    pub(crate) target: Option<String>,
    pub(crate) meta: Value,
    pub(crate) data: Vec<u8>,
}

pub(crate) type SessionRef = Arc<tokio::sync::Mutex<Session>>;

pub(crate) struct State {
    pub(crate) files: BTreeMap<String, FileRec>,
    pub(crate) root_id: String,
    pub(crate) email: String,
    clock_ms: i64,
    change_seq: u64,
    changes: Vec<(u64, String)>,
    pub(crate) sessions: HashMap<String, SessionRef>,
    codes: HashMap<String, String>,
    access_tokens: HashMap<String, i64>,
    refresh_tokens: HashSet<String>,
    pub(crate) token_lifetime_s: i64,
    pub(crate) fault: Option<StatusFault>,
    pub(crate) request_log: Vec<String>,
    pub(crate) downloaded: u64,
}

impl State {
    fn new() -> State {
        let mut state = State {
            files: BTreeMap::new(),
            root_id: String::new(),
            email: "teste@example.com".into(),
            clock_ms: 0,
            change_seq: 0,
            changes: vec![],
            sessions: HashMap::new(),
            codes: HashMap::new(),
            access_tokens: HashMap::new(),
            refresh_tokens: HashSet::new(),
            token_lifetime_s: 3599,
            fault: None,
            request_log: vec![],
            downloaded: 0,
        };
        state.root_id = state.add("", "My Drive", FOLDER_MIME, Vec::new(), None);
        state.file_mut(&state.root_id.clone()).parents.clear();
        state
    }

    /// Relógio que sempre avança, para a ordem de criação ser estável.
    fn tick(&mut self) -> i64 {
        self.clock_ms = (self.clock_ms + 1).max(now_ms());
        self.clock_ms
    }

    pub(crate) fn resolve<'a>(&'a self, id: &'a str) -> &'a str {
        if id == "root" { &self.root_id } else { id }
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
        let id = random_id(28);
        let now = self.tick();
        let md5 = md5_hex(&content);
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
                shortcut_target: target,
            },
        );
        self.record_change(&id);
        id
    }

    /// Troca o conteúdo: nova revisão, data de modificação e uma mudança no histórico.
    pub(crate) fn set_content(&mut self, id: &str, content: Vec<u8>, modified_ms: Option<i64>) {
        let now = self.tick();
        let file = self.file_mut(id);
        file.md5 = md5_hex(&content);
        file.content = content.into();
        file.head_revision = random_id(20);
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

    /// Cria um arquivo a partir dos metadados de `files.create` ou de um upload.
    pub(crate) fn create(&mut self, meta: &Value, content: Vec<u8>) -> String {
        let parent = meta["parents"][0]
            .as_str()
            .unwrap_or(&self.root_id)
            .to_string();
        let mime = meta["mimeType"]
            .as_str()
            .unwrap_or("application/octet-stream")
            .to_string();
        let target = meta["shortcutDetails"]["targetId"]
            .as_str()
            .map(String::from);
        let name = meta["name"].as_str().unwrap_or("Sem título").to_string();
        let id = self.add(&parent, &name, &mime, content, target);
        self.apply_meta(&id, meta);
        id
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
    pub(crate) fn file_json(&self, id: &str) -> Option<Value> {
        self.files.get(self.resolve(id)).map(|f| self.to_json(f))
    }

    pub(crate) fn start_page_token(&self) -> String {
        (self.change_seq + 1).to_string()
    }

    /// Mudanças a partir de `token`, uma por arquivo (o estado atual), em páginas.
    pub(crate) fn changes_since(&self, token: u64, page_size: usize) -> Value {
        let mut seen = HashSet::new();
        let pending: Vec<_> = self
            .changes
            .iter()
            .filter(|(seq, _)| *seq >= token)
            .filter(|(_, id)| seen.insert(id.as_str()))
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

    pub(crate) fn issue_code(&mut self, challenge: &str) -> String {
        let code = random_id(32);
        self.codes.insert(code.clone(), challenge.into());
        code
    }

    /// Troca o código por tokens se o `code_verifier` bater com o desafio S256.
    pub(crate) fn exchange_code(&mut self, code: &str, verifier: &str) -> Option<Value> {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if self.codes.get(code) != Some(&challenge) {
            return None;
        }
        self.codes.remove(code);
        let refresh = random_id(40);
        self.refresh_tokens.insert(refresh.clone());
        let mut tokens = self.new_access_token();
        tokens["refresh_token"] = refresh.into();
        Some(tokens)
    }

    pub(crate) fn refresh(&mut self, refresh_token: &str) -> Option<Value> {
        self.refresh_tokens
            .contains(refresh_token)
            .then(|| self.new_access_token())
    }

    fn new_access_token(&mut self) -> Value {
        let token = random_id(40);
        self.access_tokens
            .insert(token.clone(), now_ms() + self.token_lifetime_s * 1000);
        json!({
            "access_token": token,
            "expires_in": self.token_lifetime_s,
            "scope": "https://www.googleapis.com/auth/drive",
            "token_type": "Bearer",
        })
    }

    pub(crate) fn token_is_valid(&self, token: &str) -> bool {
        token == crate::TEST_ACCESS_TOKEN
            || self
                .access_tokens
                .get(token)
                .is_some_and(|&exp| exp > now_ms())
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
