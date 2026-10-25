//! Rotas HTTP do Drive simulado: Drive API v3, upload e OAuth.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Redirect, Response};
use axum::routing::{get, patch, post, put};
use futures_util::StreamExt;
use serde_json::{Value, json};

use crate::state::{self, Clause, IdRefused, Session, SessionRef, Shared, parse_query, random_id};

type Shr = State<Arc<Shared>>;
type Params = Query<HashMap<String, String>>;

/// Rotas em três grupos: a API exige token; a continuação do upload retomável não (a URL da
/// sessão já é a credencial), mas sofre as mesmas falhas; o OAuth não sofre nenhuma das duas.
pub(crate) fn router(shared: Arc<Shared>) -> Router {
    Router::new()
        .route("/drive/v3/about", get(about))
        .route("/drive/v3/files", get(list).post(create))
        .route("/drive/v3/files/generateIds", get(generate_ids))
        .route("/drive/v3/files/{id}", get(get_file).patch(update))
        .route("/drive/v3/changes/startPageToken", get(start_page_token))
        .route("/drive/v3/changes", get(changes))
        .route("/upload/drive/v3/files/{id}", patch(upload_update))
        .route("/upload/drive/v3/files", post(upload_create))
        .route("/oauth2/v3/userinfo", get(userinfo))
        .route_layer(middleware::from_fn_with_state(
            shared.clone(),
            require_token,
        ))
        .route("/upload/drive/v3/session", put(upload_chunk))
        .route_layer(middleware::from_fn_with_state(
            shared.clone(),
            injected_status,
        ))
        .route("/token", post(token))
        .route("/photo.png", get(photo))
        .route("/o/oauth2/auth", get(authorize))
        .layer(middleware::from_fn_with_state(shared.clone(), log_request))
        .with_state(shared)
}

/// Erro no formato da Drive API.
fn api_error(code: u16, reason: &str, message: &str) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({ "error": { "code": code, "message": message, "errors": [{ "domain": "global", "reason": reason, "message": message }] } });
    (status, Json(body)).into_response()
}

fn refused(e: IdRefused) -> Response {
    match e {
        IdRefused::Duplicate => api_error(
            409,
            "duplicate",
            "A file already exists with the provided ID.",
        ),
        IdRefused::NotUsable => api_error(400, "invalid", "The provided file ID is not usable."),
    }
}

fn not_found(id: &str) -> Response {
    api_error(404, "notFound", &format!("File not found: {id}."))
}

fn oauth_error(error: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response()
}

async fn log_request(State(shared): Shr, req: Request, next: Next) -> Response {
    let line = req
        .uri()
        .path_and_query()
        .map_or_else(|| req.uri().path().to_string(), |pq| pq.to_string());
    {
        let mut state = shared.state();
        state.request_log.push(line);
        state.method_log.push(req.method().to_string());
    }
    next.run(req).await
}

/// Devolve o erro injetado com `Fault::RateLimit403`/`Fault::Status`, enquanto durar, e perde
/// a resposta das criações com `Fault::LoseCreateResponses`.
async fn injected_status(State(shared): Shr, req: Request, next: Next) -> Response {
    let injected = {
        let mut state = shared.state();
        let fault = state.fault.as_mut().filter(|f| f.times > 0).map(|f| {
            f.times -= 1;
            api_error(f.code, f.reason, "Falha injetada")
        });
        if state.fault.as_ref().is_some_and(|f| f.times == 0) {
            state.fault = None;
        }
        fault
    };
    if let Some(resp) = injected {
        return resp;
    }
    let create = req.method() == axum::http::Method::POST;
    let resp = next.run(req).await;
    if create {
        let mut state = shared.state();
        if state.lose_responses > 0 {
            state.lose_responses -= 1;
            // O pedido foi processado; a conexão cai antes de a resposta sair.
            shared.cut_next_write.store(true, Ordering::SeqCst);
        }
    }
    resp
}

async fn require_token(State(shared): Shr, req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let valid = token.is_some_and(|t| shared.state().token_is_valid(t));
    if valid {
        next.run(req).await
    } else {
        api_error(401, "authError", "Invalid Credentials")
    }
}

fn page_size(params: &HashMap<String, String>) -> usize {
    params
        .get("pageSize")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100usize)
        .clamp(1, 1000)
}

/// Metadados JSON do corpo; corpo vazio vale `{}`.
fn json_body(body: &[u8]) -> Option<Value> {
    if body.is_empty() {
        Some(json!({}))
    } else {
        serde_json::from_slice(body).ok()
    }
}

fn bad_json() -> Response {
    api_error(400, "parseError", "JSON inválido")
}

async fn about(State(shared): Shr) -> Json<Value> {
    let state = shared.state();
    let usage: usize = state.files.values().map(|f| f.content.len()).sum();
    Json(json!({
        "user": {
            "emailAddress": state.email,
            "displayName": "Pessoa de Teste",
            "photoLink": shared.origin.join("photo.png").unwrap().to_string(),
        },
        "storageQuota": { "limit": "16106127360", "usage": usage.to_string() },
    }))
}

async fn list(State(shared): Shr, Query(params): Params) -> Response {
    let clauses = match parse_query(params.get("q").map_or("", String::as_str)) {
        Ok(c) => c,
        Err(e) => return api_error(400, "invalid", &e),
    };
    let size = page_size(&params);
    let offset: usize = params
        .get("pageToken")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // O compartilhado comigo só aparece quando pedido.
    let shared_only = clauses.iter().any(|c| matches!(c, Clause::SharedWithMe));
    let mut state = shared.state();
    let mut hits = state
        .files
        .values()
        .filter(|f| f.id != state.root_id && (shared_only || f.shared.is_none()))
        .filter(|f| clauses.iter().all(|c| c.matches(f)))
        .skip(offset);
    let files: Vec<Value> = hits.by_ref().take(size).map(|f| state.to_json(f)).collect();
    let mut body = json!({ "kind": "drive#fileList", "files": files });
    if hits.next().is_some() {
        body["nextPageToken"] = (offset + size).to_string().into();
    }
    if let Some((pages, ..)) = &mut state.edit_during_listing {
        *pages -= 1;
        if *pages == 0 {
            let (_, id, bytes) = state.edit_during_listing.take().unwrap();
            state.set_content(&id, bytes, None);
        }
    }
    Json(body).into_response()
}

async fn get_file(
    State(shared): Shr,
    Path(id): Path<String>,
    Query(params): Params,
    headers: HeaderMap,
) -> Response {
    let mut state = shared.state();
    if params.get("alt").map(String::as_str) != Some("media") {
        return state
            .file_json(&id)
            .map_or_else(|| not_found(&id), |v| Json(v).into_response());
    }
    let Some(file) = state.files.get(state.resolve(&id)) else {
        return not_found(&id);
    };
    if !file.has_content() {
        return api_error(
            403,
            "fileNotDownloadable",
            "Only files with binary content can be downloaded.",
        );
    }
    let content = file.content.clone();
    let len = content.len() as u64;
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_range);
    let (start, end) = match range {
        None => (0, len),
        Some((start, _)) if start >= len => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{len}"))],
            )
                .into_response();
        }
        Some((start, last)) => (start, last.map_or(len, |l| (l + 1).min(len))),
    };
    state.downloaded += end - start;
    drop(state);
    let bytes = content.slice(start as usize..end as usize);
    match range {
        None => bytes.into_response(),
        Some(_) => {
            let content_range = format!("bytes {start}-{}/{len}", end - 1);
            (
                StatusCode::PARTIAL_CONTENT,
                [(header::CONTENT_RANGE, content_range)],
                bytes,
            )
                .into_response()
        }
    }
}

/// `bytes=início-fim` (fim inclusivo e opcional).
fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    Some((
        start.parse().ok()?,
        if end.is_empty() {
            None
        } else {
            Some(end.parse().ok()?)
        },
    ))
}

async fn create(State(shared): Shr, body: Bytes) -> Response {
    let Some(meta) = json_body(&body) else {
        return bad_json();
    };
    let mut state = shared.state();
    match state.create(&meta, Vec::new()) {
        Ok(id) => Json(state.to_json(&state.files[&id])).into_response(),
        Err(e) => refused(e),
    }
}

async fn generate_ids(State(shared): Shr, Query(params): Params) -> Response {
    let count = params
        .get("count")
        .and_then(|c| c.parse().ok())
        .unwrap_or(10usize)
        .min(1000);
    Json(json!({ "kind": "drive#generatedIds", "space": "drive", "ids": shared.state().generate_ids(count) }))
        .into_response()
}

async fn update(
    State(shared): Shr,
    Path(id): Path<String>,
    Query(params): Params,
    body: Bytes,
) -> Response {
    let Some(meta) = json_body(&body) else {
        return bad_json();
    };
    let mut state = shared.state();
    let id = state.resolve(&id).to_string();
    let Some(file) = state.files.get_mut(&id) else {
        return not_found(&id);
    };
    let ids = |key: &str| {
        params
            .get(key)
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let remove = ids("removeParents");
    file.parents.retain(|p| !remove.contains(&p.as_str()));
    file.parents
        .extend(ids("addParents").into_iter().map(String::from));
    state.apply_meta(&id, &meta);
    Json(state.to_json(&state.files[&id])).into_response()
}

async fn start_page_token(State(shared): Shr) -> Json<Value> {
    Json(json!({ "startPageToken": shared.state().start_page_token() }))
}

async fn changes(State(shared): Shr, Query(params): Params) -> Response {
    let Some(token) = params.get("pageToken").and_then(|t| t.parse().ok()) else {
        return api_error(400, "required", "pageToken obrigatório");
    };
    Json(shared.state().changes_since(token, page_size(&params))).into_response()
}

/// Separa um corpo `multipart/related` em metadados (JSON) e conteúdo.
fn parse_multipart(headers: &HeaderMap, body: &[u8]) -> Option<(Value, Vec<u8>)> {
    let content_type = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
    let boundary = content_type
        .split("boundary=")
        .nth(1)?
        .split(';')
        .next()?
        .trim_matches('"');
    let delimiter = format!("--{boundary}");
    let mut parts = vec![];
    let mut rest = &body[find(body, delimiter.as_bytes())? + delimiter.len()..];
    while !rest.starts_with(b"--") {
        let next = find(rest, delimiter.as_bytes())?;
        let part = rest[..next].strip_prefix(b"\r\n")?;
        let part = part.strip_suffix(b"\r\n").unwrap_or(part);
        let headers_end = find(part, b"\r\n\r\n")?;
        parts.push(&part[headers_end + 4..]);
        rest = &rest[next + delimiter.len()..];
    }
    let meta = serde_json::from_slice(parts.first()?).ok()?;
    Some((meta, parts.get(1).map_or_else(Vec::new, |c| c.to_vec())))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Upload que cria arquivo (`POST /upload/drive/v3/files`).
async fn upload_create(
    State(shared): Shr,
    Query(params): Params,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    upload_start(&shared, None, &params, &headers, body)
}

/// Upload que troca o conteúdo (`PATCH /upload/drive/v3/files/{id}`).
async fn upload_update(
    State(shared): Shr,
    Path(id): Path<String>,
    Query(params): Params,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    upload_start(&shared, Some(id), &params, &headers, body)
}

fn upload_start(
    shared: &Shared,
    target: Option<String>,
    params: &HashMap<String, String>,
    headers: &HeaderMap,
    body: Bytes,
) -> Response {
    let (meta, content) = match params.get("uploadType").map(String::as_str) {
        Some("multipart") => match parse_multipart(headers, &body) {
            Some(parts) => parts,
            None => return api_error(400, "badContent", "multipart inválido"),
        },
        Some("media") => (json!({}), body.to_vec()),
        Some("resumable") => match json_body(&body) {
            Some(meta) => return start_resumable(shared, target, meta),
            None => return bad_json(),
        },
        _ => return api_error(400, "invalid", "uploadType inválido"),
    };
    finish_upload(&mut shared.state(), target, &meta, content)
}

/// Abre uma sessão retomável e devolve a URL dela no `Location`.
fn start_resumable(shared: &Shared, target: Option<String>, meta: Value) -> Response {
    let mut state = shared.state();
    if let Some(id) = &target
        && !state.files.contains_key(id)
    {
        return not_found(id);
    }
    if target.is_none()
        && let Err(e) = state.check_requested_id(&meta)
    {
        return refused(e);
    }
    let sid = random_id(32);
    let location = shared
        .origin
        .join(&format!("upload/drive/v3/session?upload_id={sid}"))
        .unwrap();
    state.sessions.insert(
        sid,
        Arc::new(tokio::sync::Mutex::new(Session {
            target,
            meta,
            data: vec![],
            owner: 0,
        })),
    );
    (StatusCode::OK, [(header::LOCATION, location.to_string())]).into_response()
}

fn finish_upload(
    state: &mut state::State,
    target: Option<String>,
    meta: &Value,
    content: Vec<u8>,
) -> Response {
    let id = match target {
        Some(id) if !state.files.contains_key(&id) => return not_found(&id),
        Some(id) => {
            let modified = meta["modifiedTime"].as_str().and_then(state::parse_rfc3339);
            state.set_content(&id, content, modified);
            state.apply_meta(&id, meta);
            id
        }
        None => match state.create(meta, content) {
            Ok(id) => id,
            Err(e) => return refused(e),
        },
    };
    Json(state.to_json(&state.files[&id])).into_response()
}

/// `Content-Range: bytes a-b/total` ou `bytes */total` (consulta de offset).
fn parse_content_range(value: &str) -> Option<(Option<u64>, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let total = total.parse().ok()?;
    let start = if range == "*" {
        None
    } else {
        Some(range.split_once('-')?.0.parse().ok()?)
    };
    Some((start, total))
}

/// Envio de um trecho (ou consulta de offset) numa sessão retomável. O que chegar antes de a
/// conexão cair fica guardado, como no Drive real.
async fn upload_chunk(
    State(shared): Shr,
    Query(params): Params,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let session = params
        .get("upload_id")
        .and_then(|sid| Some((sid.clone(), shared.state().sessions.get(sid)?.clone())));
    let Some((sid, entry)) = session else {
        return api_error(404, "notFound", "sessão de upload não encontrada");
    };
    let mut session = entry.lock().await;
    session.owner += 1;
    let me = session.owner;
    let content_range = headers
        .get(header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok());
    let (start, total) = match content_range.map(parse_content_range) {
        Some(Some(parsed)) => parsed,
        Some(None) => return api_error(400, "invalid", "Content-Range inválido"),
        None => {
            let len = headers
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse().ok())
                .unwrap_or(0);
            (Some(0), len)
        }
    };
    if let Some(start) = start {
        if start as usize > session.data.len() {
            return api_error(400, "invalid", "trecho fora de ordem");
        }
        session.data.truncate(start as usize);
        // A trava sai durante a leitura do corpo, que pode nunca terminar.
        drop(session);
        let mut stream = body.into_data_stream();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else {
                return api_error(400, "badContent", "conexão interrompida");
            };
            let Some(mut session) = owned(&entry, me).await else {
                return superseded();
            };
            session.data.extend_from_slice(&chunk);
        }
        let Some(owner) = owned(&entry, me).await else {
            return superseded();
        };
        session = owner;
    }
    let received = session.data.len() as u64;
    if received < total {
        let mut resp = StatusCode::PERMANENT_REDIRECT.into_response();
        if received > 0 {
            resp.headers_mut().insert(
                header::RANGE,
                format!("bytes=0-{}", received - 1).parse().unwrap(),
            );
        }
        return resp;
    }
    let mut state = shared.state();
    state.sessions.remove(&sid);
    let data = std::mem::take(&mut session.data);
    finish_upload(&mut state, session.target.take(), &session.meta, data)
}

/// A sessão, se `me` ainda for a requisição dona dela.
async fn owned(entry: &SessionRef, me: u64) -> Option<tokio::sync::MutexGuard<'_, Session>> {
    let session = entry.lock().await;
    (session.owner == me).then_some(session)
}

fn superseded() -> Response {
    api_error(400, "badContent", "substituída por outra requisição")
}

async fn token(
    State(shared): Shr,
    axum::Form(form): axum::Form<HashMap<String, String>>,
) -> Response {
    let field = |k: &str| form.get(k).map_or("", String::as_str);
    let mut state = shared.state();
    let tokens = match field("grant_type") {
        "authorization_code" => state.exchange_code(field("code"), field("code_verifier")),
        "refresh_token" => state.refresh(field("refresh_token")),
        _ => return oauth_error("unsupported_grant_type"),
    };
    tokens.map_or_else(|| oauth_error("invalid_grant"), |t| Json(t).into_response())
}

/// Consentimento simulado: aprova na hora e devolve o código ao `redirect_uri`.
async fn authorize(State(shared): Shr, Query(params): Params) -> Response {
    let (Some(redirect), Some(challenge)) =
        (params.get("redirect_uri"), params.get("code_challenge"))
    else {
        return oauth_error("invalid_request");
    };
    let Ok(mut target) = url::Url::parse(redirect) else {
        return oauth_error("invalid_request");
    };
    let code = shared.state().issue_code(challenge);
    target.query_pairs_mut().append_pair("code", &code);
    if let Some(state) = params.get("state") {
        target.query_pairs_mut().append_pair("state", state);
    }
    Redirect::to(target.as_str()).into_response()
}

/// PNG de 1×1 pixel, a foto da conta de teste.
async fn photo() -> Response {
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x60,
        0x60, 0xf8, 0x0f, 0x00, 0x01, 0x04, 0x01, 0x00, 0x5f, 0xe5, 0xc3, 0x4b, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    ([(axum::http::header::CONTENT_TYPE, "image/png")], PNG).into_response()
}

async fn userinfo(State(shared): Shr) -> Json<Value> {
    Json(json!({ "email": shared.state().email, "email_verified": true }))
}
