//! Rotas de permissões (`files/{id}/permissions`) e de revisões (`files/{id}/revisions`) do Drive
//! simulado.

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use serde_json::{Value, json};

use crate::server::{Params, Shr, api_error, bad_json, json_body, not_found};
use crate::state::{FileRec, Shared, random_id, rfc3339};

pub(crate) fn routes() -> Router<Arc<Shared>> {
    Router::new()
        .route(
            "/drive/v3/files/{id}/permissions",
            get(list_permissions).post(create_permission),
        )
        .route(
            "/drive/v3/files/{id}/permissions/{pid}",
            axum::routing::patch(update_permission).delete(delete_permission),
        )
        .route("/drive/v3/files/{id}/revisions", get(list_revisions))
        .route(
            "/drive/v3/files/{id}/revisions/{rid}",
            get(get_revision).patch(update_revision),
        )
}

/// Roda `act` sobre o item, se ele existe e o usuário pode editá-lo: só editores mexem nas
/// permissões e nas versões (como o padrão do Drive).
fn with_editable(
    shared: &Shared,
    id: &str,
    act: impl FnOnce(&mut FileRec) -> Response,
) -> Response {
    let mut state = shared.state();
    let resolved = state.resolve(id).to_string();
    match state.files.get_mut(&resolved) {
        None => not_found(id),
        Some(file) if file.can_edit => act(file),
        Some(_) => api_error(
            403,
            "insufficientFilePermissions",
            "The user does not have sufficient permissions for this file.",
        ),
    }
}

async fn list_permissions(State(shared): Shr, Path(id): Path<String>) -> Response {
    let state = shared.state();
    match state.files.get(state.resolve(&id)) {
        Some(file) => Json(json!({ "permissions": file.permissions })).into_response(),
        None => not_found(&id),
    }
}

async fn create_permission(
    State(shared): Shr,
    Path(id): Path<String>,
    axum::extract::Query(params): Params,
    body: Bytes,
) -> Response {
    let Some(meta) = json_body(&body) else {
        return bad_json();
    };
    let kind = meta["type"].as_str().unwrap_or_default();
    // O campo que diz a quem a permissão dá acesso (nenhum para "qualquer pessoa").
    let field = match kind {
        "user" | "group" => Some("emailAddress"),
        "domain" => Some("domain"),
        "anyone" => None,
        _ => return api_error(400, "invalid", "Unknown permission type."),
    };
    let target = field.map(|f| meta[f].as_str());
    let (Some(role), Some(target)) = (meta["role"].as_str(), target.unwrap_or(Some(""))) else {
        return api_error(400, "invalid", "Permission target or role is missing.");
    };
    let resp = with_editable(&shared, &id, |file| {
        let pid = if kind == "anyone" {
            "anyoneWithLink".into()
        } else {
            random_id(20)
        };
        let mut perm = json!({ "id": pid, "type": kind, "role": role });
        if let Some(field) = field {
            perm[field] = target.into();
        }
        file.permissions.push(perm.clone());
        Json(perm).into_response()
    });
    let notify = params.get("sendNotificationEmail").map(String::as_str) != Some("false");
    if resp.status().is_success() && notify && field == Some("emailAddress") {
        let message = params.get("emailMessage").cloned();
        shared
            .state()
            .share_emails
            .push((target.to_string(), message));
    }
    resp
}

async fn update_permission(
    State(shared): Shr,
    Path((id, pid)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let Some(role) = json_body(&body).and_then(|m| m["role"].as_str().map(String::from)) else {
        return bad_json();
    };
    with_editable(&shared, &id, |file| {
        match file
            .permissions
            .iter_mut()
            .find(|p| p["id"] == pid.as_str())
        {
            Some(p) => {
                p["role"] = role.into();
                Json(p.clone()).into_response()
            }
            None => permission_not_found(&pid),
        }
    })
}

fn permission_not_found(pid: &str) -> Response {
    api_error(404, "notFound", &format!("Permission not found: {pid}."))
}

async fn delete_permission(
    State(shared): Shr,
    Path((id, pid)): Path<(String, String)>,
) -> Response {
    with_editable(&shared, &id, |file| {
        let Some(pos) = file
            .permissions
            .iter()
            .position(|p| p["id"] == pid.as_str())
        else {
            return permission_not_found(&pid);
        };
        if file.permissions[pos]["role"] == "owner" {
            let message = "The owner of a file cannot be removed.";
            return api_error(403, "cannotRemoveOwner", message);
        }
        file.permissions.remove(pos);
        axum::http::StatusCode::NO_CONTENT.into_response()
    })
}

/// O conteúdo e a data de uma versão do item (as antigas ou a atual).
fn find_revision(file: &FileRec, rid: &str) -> Option<(Bytes, i64)> {
    match file.old_revisions.iter().find(|r| r.id == rid) {
        Some(r) => Some((r.content.clone(), r.modified_ms)),
        None => (file.head_revision == rid).then(|| (file.content.clone(), file.modified_ms)),
    }
}

fn revision_json(file: &FileRec, rid: &str, size: usize, modified_ms: i64) -> Value {
    json!({
        "id": rid,
        "modifiedTime": rfc3339(modified_ms),
        "size": size.to_string(),
        "keepForever": file.keep_forever.contains(rid),
    })
}

fn revision_not_found(rid: &str) -> Response {
    api_error(404, "notFound", &format!("Revision not found: {rid}."))
}

async fn list_revisions(State(shared): Shr, Path(id): Path<String>) -> Response {
    let state = shared.state();
    let Some(file) = state.files.get(state.resolve(&id)) else {
        return not_found(&id);
    };
    let old = file
        .old_revisions
        .iter()
        .map(|r| (&r.id, &r.content, r.modified_ms));
    let head = (&file.head_revision, &file.content, file.modified_ms);
    let list: Vec<Value> = old
        .chain([head])
        .map(|(rid, content, modified_ms)| revision_json(file, rid, content.len(), modified_ms))
        .collect();
    Json(json!({ "revisions": list })).into_response()
}

async fn get_revision(
    State(shared): Shr,
    Path((id, rid)): Path<(String, String)>,
    axum::extract::Query(params): Params,
) -> Response {
    let mut state = shared.state();
    let Some(file) = state.files.get(state.resolve(&id)) else {
        return not_found(&id);
    };
    let Some((content, modified_ms)) = find_revision(file, &rid) else {
        return revision_not_found(&rid);
    };
    if params.get("alt").map(String::as_str) != Some("media") {
        return Json(revision_json(file, &rid, content.len(), modified_ms)).into_response();
    }
    state.downloaded += content.len() as u64;
    content.into_response()
}

async fn update_revision(
    State(shared): Shr,
    Path((id, rid)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let Some(keep) = json_body(&body).and_then(|m| m["keepForever"].as_bool()) else {
        return bad_json();
    };
    with_editable(&shared, &id, |file| {
        let Some((content, modified_ms)) = find_revision(file, &rid) else {
            return revision_not_found(&rid);
        };
        match keep {
            true => file.keep_forever.insert(rid.clone()),
            false => file.keep_forever.remove(&rid),
        };
        Json(revision_json(file, &rid, content.len(), modified_ms)).into_response()
    })
}
