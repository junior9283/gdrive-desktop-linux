//! Testes de contrato do Drive simulado, falando HTTP direto com `reqwest`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use gdl_fake_drive::{FakeDrive, Fault, TEST_ACCESS_TOKEN};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn client() -> Client {
    Client::new()
}

#[tokio::test]
async fn range_request_returns_206_with_exact_bytes() {
    let fake = FakeDrive::start().await;
    let data: Vec<u8> = (0..=255).collect();
    let id = fake.add_file(&fake.root_id(), "a.bin", data);
    let resp = client()
        .get(
            fake.base_url()
                .join(&format!("files/{id}?alt=media"))
                .unwrap(),
        )
        .bearer_auth(TEST_ACCESS_TOKEN)
        .header("Range", "bytes=10-19")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(resp.headers()["content-range"], "bytes 10-19/256");
    assert_eq!(
        resp.bytes().await.unwrap().as_ref(),
        &(10..=19).collect::<Vec<u8>>()[..]
    );
    assert_eq!(fake.downloaded_bytes(), 10);
}

#[tokio::test]
async fn requests_without_valid_token_get_401() {
    let fake = FakeDrive::start().await;
    let url = fake.base_url().join("about").unwrap();
    assert_eq!(
        client().get(url.clone()).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let resp = client()
        .get(url)
        .bearer_auth("inventado")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn resumable_upload_can_resume_after_drop() {
    let fake = FakeDrive::start().await;
    let data: Vec<u8> = (0..100_000u32).map(|n| n as u8).collect();
    let resp = client()
        .post(
            fake.upload_url()
                .join("files?uploadType=resumable")
                .unwrap(),
        )
        .bearer_auth(TEST_ACCESS_TOKEN)
        .header("X-Upload-Content-Length", data.len())
        .json(&json!({ "name": "grande.bin", "parents": [fake.root_id()] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let session = resp.headers()["location"].to_str().unwrap().to_string();

    fake.inject(Fault::DropConnectionAfterBytes(50_000));
    let first = client()
        .put(&session)
        .header("Content-Range", "bytes 0-99999/100000")
        .body(data.clone())
        .send()
        .await;
    assert!(first.is_err(), "a conexão devia cair: {first:?}");

    let status = client()
        .put(&session)
        .header("Content-Range", "bytes */100000")
        .send()
        .await
        .unwrap();
    assert_eq!(status.status().as_u16(), 308);
    let range = status.headers()["range"].to_str().unwrap();
    let received: usize = range
        .strip_prefix("bytes=0-")
        .unwrap()
        .parse::<usize>()
        .unwrap()
        + 1;
    assert!(
        received > 0 && received < data.len(),
        "recebidos: {received}"
    );

    let done = client()
        .put(&session)
        .header("Content-Range", format!("bytes {received}-99999/100000"))
        .body(data[received..].to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(done.status(), StatusCode::OK);
    let file: Value = done.json().await.unwrap();
    assert_eq!(fake.content(file["id"].as_str().unwrap()), data);
}

#[tokio::test]
async fn multipart_upload_creates_file_with_metadata() {
    let fake = FakeDrive::start().await;
    let body = format!(
        "--b\r\nContent-Type: application/json\r\n\r\n{}\r\n--b\r\nContent-Type: text/plain\r\n\r\nolá\r\n--b--\r\n",
        json!({ "name": "nota.txt", "parents": [fake.root_id()] })
    );
    let resp = client()
        .post(
            fake.upload_url()
                .join("files?uploadType=multipart")
                .unwrap(),
        )
        .bearer_auth(TEST_ACCESS_TOKEN)
        .header("Content-Type", "multipart/related; boundary=b")
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let file: Value = resp.json().await.unwrap();
    let id = file["id"].as_str().unwrap();
    assert_eq!(fake.content(id), "olá".as_bytes());
    assert_eq!(fake.meta(id)["name"], "nota.txt");
    assert_eq!(fake.meta(id)["size"], "4");
    assert_eq!(
        fake.requests("/upload/drive/v3/files?uploadType=multipart"),
        1
    );
}

#[tokio::test]
async fn list_filters_by_parent_and_trashed_and_paginates() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let folder = fake.add_folder(&root, "Pasta");
    for n in 0..5 {
        fake.add_file(&folder, &format!("f{n}"), vec![]);
    }
    let gone = fake.add_file(&folder, "lixo", vec![]);
    fake.trash_remote(&gone);
    let mut names = vec![];
    let mut token: Option<String> = None;
    loop {
        let mut req = client()
            .get(fake.base_url().join("files").unwrap())
            .bearer_auth(TEST_ACCESS_TOKEN)
            .query(&[
                ("q", format!("'{folder}' in parents and trashed = false")),
                ("pageSize", "2".into()),
            ]);
        if let Some(t) = &token {
            req = req.query(&[("pageToken", t)]);
        }
        let page: Value = req.send().await.unwrap().json().await.unwrap();
        names.extend(
            page["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["name"].as_str().unwrap().to_string()),
        );
        match page["nextPageToken"].as_str() {
            Some(t) => token = Some(t.into()),
            None => break,
        }
    }
    names.sort();
    assert_eq!(names, ["f0", "f1", "f2", "f3", "f4"]);
}

#[tokio::test]
async fn changes_list_reports_remote_modification_and_trash() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let edited = fake.add_file(&root, "editado.txt", b"v1".to_vec());
    let trashed = fake.add_file(&root, "apagado.txt", b"x".to_vec());
    let old_rev = fake.meta(&edited)["headRevisionId"].clone();
    let get = |path: &str| {
        client()
            .get(fake.base_url().join(path).unwrap())
            .bearer_auth(TEST_ACCESS_TOKEN)
            .send()
    };
    let start: Value = get("changes/startPageToken")
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = start["startPageToken"].as_str().unwrap().to_string();

    fake.modify_remote(&edited, b"v2".to_vec());
    fake.trash_remote(&trashed);

    let page: Value = get(&format!("changes?pageToken={token}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let changes = page["changes"].as_array().unwrap();
    let find = |id: &str| {
        changes
            .iter()
            .find(|c| c["fileId"] == id)
            .unwrap_or_else(|| panic!("sem mudança para {id}"))
    };
    assert_ne!(find(&edited)["file"]["headRevisionId"], old_rev);
    assert_eq!(find(&trashed)["file"]["trashed"], true);
    let next = page["newStartPageToken"].as_str().unwrap();
    let empty: Value = get(&format!("changes?pageToken={next}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(empty["changes"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn pkce_verifier_mismatch_is_rejected() {
    let fake = FakeDrive::start().await;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(b"verificador-certo"));
    let code = fake.issue_code(&challenge);
    let exchange = |verifier: &'static str| {
        client()
            .post(fake.token_url())
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("code_verifier", verifier),
                ("client_id", "id"),
                ("redirect_uri", "http://127.0.0.1:1"),
            ])
            .send()
    };
    let bad = exchange("verificador-errado").await.unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    assert_eq!(bad.json::<Value>().await.unwrap()["error"], "invalid_grant");

    let good = exchange("verificador-certo").await.unwrap();
    assert_eq!(good.status(), StatusCode::OK);
    let tokens: Value = good.json().await.unwrap();
    let access = tokens["access_token"].as_str().unwrap();
    assert!(tokens["refresh_token"].is_string());
    let about = client()
        .get(fake.base_url().join("about").unwrap())
        .bearer_auth(access)
        .send()
        .await
        .unwrap();
    assert_eq!(about.status(), StatusCode::OK);

    assert_eq!(
        exchange("verificador-certo").await.unwrap().status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn rate_limit_fault_returns_403_user_rate_limit_exceeded_then_succeeds() {
    let fake = FakeDrive::start().await;
    fake.inject(Fault::RateLimit403 { times: 2 });
    let about = || {
        client()
            .get(fake.base_url().join("about").unwrap())
            .bearer_auth(TEST_ACCESS_TOKEN)
            .send()
    };
    for _ in 0..2 {
        let resp = about().await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let err: Value = resp.json().await.unwrap();
        assert_eq!(err["error"]["errors"][0]["reason"], "userRateLimitExceeded");
    }
    let ok: Value = about().await.unwrap().json().await.unwrap();
    assert_eq!(ok["user"]["emailAddress"], fake.email());
}

#[tokio::test]
async fn offline_refuses_requests_until_cleared() {
    let fake = FakeDrive::start().await;
    let url = fake.base_url().join("about").unwrap();
    fake.inject(Fault::Offline);
    assert!(
        client()
            .get(url.clone())
            .bearer_auth(TEST_ACCESS_TOKEN)
            .send()
            .await
            .is_err()
    );
    fake.clear_faults();
    assert_eq!(
        client()
            .get(url)
            .bearer_auth(TEST_ACCESS_TOKEN)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn revoked_account_loses_access_and_refresh() {
    let fake = FakeDrive::start().await;
    let ana = fake.add_account("ana@example.com");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(b"verificador"));
    fake.login_as("ana@example.com");
    let code = fake.issue_code(&challenge);
    let tokens: Value = client()
        .post(fake.token_url())
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("code_verifier", "verificador"),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let about = |token: String| {
        client()
            .get(fake.base_url().join("about").unwrap())
            .query(&[("fields", "user")])
            .bearer_auth(token)
            .send()
    };
    assert_eq!(
        about(tokens["access_token"].as_str().unwrap().into())
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );

    fake.revoke(ana.email());

    assert_eq!(
        about(tokens["access_token"].as_str().unwrap().into())
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let refresh = client()
        .post(fake.token_url())
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", tokens["refresh_token"].as_str().unwrap()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn shared_drive_items_need_include_items_from_all_drives() {
    let fake = FakeDrive::start().await;
    let ana = fake.add_account("ana@example.com");
    let drive = ana.add_shared_drive("Equipe");
    ana.add_file(&drive, "dentro.txt", vec![]);
    ana.add_file(&ana.root_id(), "fora.txt", vec![]);
    let names = |extra: &str| {
        let url = fake
            .base_url()
            .join(&format!("files?pageSize=100&{extra}"))
            .unwrap();
        let token = ana.access_token();
        async move {
            let body: Value = client()
                .get(url)
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            let mut names: Vec<String> = body["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["name"].as_str().unwrap().to_string())
                .collect();
            names.sort();
            names
        }
    };

    assert_eq!(names("").await, ["fora.txt"]);
    assert_eq!(
        names("includeItemsFromAllDrives=true&supportsAllDrives=true").await,
        ["dentro.txt", "fora.txt"],
        "a raiz do drive não é um item da listagem"
    );
}
