# Miniaturas sem baixar os arquivos — plano de implementação

> **Para agentes:** SUB-SKILL OBRIGATÓRIA: use superpowers:subagent-driven-development (recomendado) ou superpowers:executing-plans para executar este plano tarefa a tarefa. Os passos usam caixas (`- [ ]`) para acompanhamento.

**Objetivo:** ao abrir uma pasta da montagem, gravar no cache de miniaturas do freedesktop as miniaturas que o Google já gera, para o Dolphin não ler os arquivos inteiros.

**Arquitetura:** a especificação do freedesktop (URI, nome do arquivo, PNG com `Thumb::*`) fica num módulo puro do `gdl-engine`; o `gdl-drive` lista os `thumbnailLink` de uma pasta e baixa a imagem; o `gdl-store` lembra o que foi semeado; o motor junta tudo em `seed_thumbnails`; o `opendir` do `gdl-fs` chama o motor e espera com prazo; o daemon passa o ponto de montagem e limpa tudo ao sair da conta.

**Tecnologias:** Rust (edição 2024), crate `image` (decodificar JPEG/PNG/WebP, reduzir, gravar PNG com blocos de texto via `png`), `md-5`, `percent-encoding`, `tokio` (JoinSet + Semaphore, timeout), `tokio-util` (CancellationToken).

**Especificação:** `docs/superpowers/specs/2026-10-26-miniaturas-design.md`

## Restrições globais

- PNG RGBA; tamanhos gravados: `normal` 128, `large` 256, `x-large` 512 (nunca `xx-large`); lado maior = o tamanho, sem ampliar.
- Texto obrigatório: `Thumb::URI` e `Thumb::MTime` (segundos inteiros = `st_mtime` da montagem); também `Thumb::Size` (tamanho do arquivo em bytes).
- Arquivos 0600, diretórios 0700, gravação atômica (temporário no mesmo diretório + rename).
- URI: `file://` + caminho absoluto com percent-encoding de tudo exceto alfanuméricos e `-._~/!$&'()*+,;=:@`.
- Nome do arquivo: MD5 hexadecimal minúsculo da URI + `.png`.
- Download da miniatura: pedido com `=s512`, no máximo 8 em paralelo, token só para `https` em `*.googleusercontent.com` / `*.google.com` ou na mesma origem da API.
- Espera do `opendir`: no máximo `THUMB_WAIT = 1500 ms`; o resto segue em segundo plano.
- Não semear: pastas fora do Meu Drive, documentos do Google, itens `local_state != Clean`, arquivos completos no cache local, arquivos sem `thumbnailLink`.
- Não sobrescrever um PNG cujo `Thumb::MTime` já é o atual.
- Testes só com o Drive simulado; nada de rede nem da conta real. Textos e comentários em pt-BR.

## Pontos de atenção da revisão

1. **Nomes com espaço, acento, `#`, `%`, `?`:** a URI precisa bater com a do KIO/GLib; teste com `Fotos de Março #1 (100%).jpg`.
2. **Sem rede / erro da API:** a listagem da pasta sai normal, sem esperar mais que o prazo e sem erro no `opendir`.
3. **Arquivo mudou na nuvem depois de semeado:** na próxima abertura, a miniatura é refeita com o `Thumb::MTime` novo.
4. **Pasta só com documentos do Google ou pastas:** nenhuma chamada à API.
5. **Duas aberturas simultâneas da mesma pasta:** uma única semeadura (sem downloads duplicados).

---

### Tarefa 1: Especificação do freedesktop (módulo puro)

**Arquivos:**
- Criar: `crates/gdl-engine/src/thumbs/mod.rs` (só `pub mod freedesktop; pub mod seeder;` — o `seeder` entra na Tarefa 4)
- Criar: `crates/gdl-engine/src/thumbs/freedesktop.rs`
- Modificar: `crates/gdl-engine/src/lib.rs` (`pub mod thumbs;`), `crates/gdl-engine/Cargo.toml` e `Cargo.toml` (deps `image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp"] }`, `png`, `md-5`, `percent-encoding`, todas pelo workspace)
- Modificar: `crates/gdl-common/src/paths.rs` (campo novo `thumbnails_dir`)

**Interfaces — produz:**
- `Paths::thumbnails_dir: PathBuf` = `$XDG_CACHE_HOME/thumbnails` (padrão `~/.cache/thumbnails`); em `for_test`, `<root>/cache/thumbnails`.
- `pub enum ThumbSize { Normal, Large, XLarge }` com `pub const ALL: [ThumbSize; 3]`, `fn dir_name(self) -> &'static str`, `fn pixels(self) -> u32`.
- `pub fn uri_of(path: &Path) -> String`
- `pub fn file_for(thumbs_dir: &Path, size: ThumbSize, uri: &str) -> PathBuf`
- `pub fn stored_mtime(file: &Path) -> Option<i64>` (lê `Thumb::MTime` de um PNG existente; `None` se não existe ou é inválido)
- `pub fn write_all(thumbs_dir: &Path, uri: &str, mtime_s: i64, file_size: u64, image: &[u8]) -> Result<(), ThumbError>` — decodifica uma vez, grava os 3 tamanhos.
- `pub fn remove_all(thumbs_dir: &Path, uri: &str)` (ignora "não existe").
- `pub enum ThumbError { Decode(String), Io(std::io::Error) }` (thiserror, como o resto do crate).

- [ ] **Passo 1: testes que falham** (em `freedesktop.rs`, `#[cfg(test)]`)

```rust
#[test]
fn uri_matches_kio_encoding() {
    assert_eq!(
        uri_of(Path::new("/home/u/Google Drive/Meu Drive/Fotos de Março #1 (100%).jpg")),
        "file:///home/u/Google%20Drive/Meu%20Drive/Fotos%20de%20Mar%C3%A7o%20%231%20(100%25).jpg"
    );
}

#[test]
fn file_name_is_md5_of_uri() {
    // MD5("file:///a/b.jpg")
    assert_eq!(
        file_for(Path::new("/c/thumbnails"), ThumbSize::Large, "file:///a/b.jpg"),
        PathBuf::from("/c/thumbnails/large/").join(format!("{:x}.png", md5::Md5::digest(b"file:///a/b.jpg")))
    );
}

#[test]
fn write_all_creates_three_sizes_with_metadata() {
    // imagem 600x400 gerada no teste (crate image) e codificada em JPEG
    // espera: normal 128x85, large 256x171, x-large 512x341; todos com
    // Thumb::URI == uri, Thumb::MTime == "1700000000", Thumb::Size == "12345";
    // permissões 0o600 nos arquivos e 0o700 nos diretórios; nenhum xx-large
}

#[test]
fn small_image_is_not_upscaled() {
    // imagem 100x80: normal tem 100x80 (lado maior <= 128 não amplia), large/x-large também 100x80
}

#[test]
fn stored_mtime_reads_back_and_ignores_garbage() {
    // depois de write_all(..., 1700000000, ...): stored_mtime(normal) == Some(1700000000)
    // arquivo inexistente -> None; arquivo com bytes aleatórios -> None
}

#[test]
fn undecodable_image_is_an_error_and_writes_nothing() {
    // write_all com b"nao e imagem" -> Err(ThumbError::Decode(_)); diretório de miniaturas sem arquivos
}

#[test]
fn remove_all_deletes_every_size() {}
```

E em `paths.rs`: `for_test_puts_everything_under_root` passa a conferir `thumbnails_dir == /tmp/x/cache/thumbnails`; um teste novo confere `XDG_CACHE_HOME` relativo ignorado (mesma regra das outras bases).

- [ ] **Passo 2:** `cargo test -p gdl-engine thumbs::freedesktop` e `cargo test -p gdl-common paths` — falham (não compila).
- [ ] **Passo 3: implementar.** Redução com `image::imageops::thumbnail` (ou `resize` com `FilterType::Triangle`); PNG com `png::Encoder` + `add_text_chunk` para os três `Thumb::*`; temporário `.<nome>.tmp-<pid>` + `rename`; diretórios com `gdl_common::paths::create_private_dir` (já cria 0700). Leitura de `stored_mtime` com `png::Decoder` só até o cabeçalho/info (`read_info`), sem decodificar a imagem.
- [ ] **Passo 4:** os mesmos comandos passam.
- [ ] **Passo 5: commit** — `Miniaturas: formato do cache do freedesktop`.

### Tarefa 2: Links de miniatura na API e no Drive simulado

**Arquivos:**
- Modificar: `crates/gdl-drive/src/client.rs` (novos métodos; extrair a regra de host do `fetch_photo`)
- Modificar: `crates/gdl-drive/src/model.rs` (struct `ThumbLink`)
- Modificar: `crates/gdl-fake-drive/src/state.rs`, `server.rs`, `lib.rs`
- Testes: os testes do cliente que já usam o `FakeDrive` (mesmo arquivo/módulo dos testes de `fetch_photo`)

**Interfaces — produz:**
- `pub struct ThumbLink { pub id: String, pub link: String }` (serde `thumbnailLink` → `link`; itens sem link são descartados na listagem)
- `DriveClient::thumbnail_links(&self, folder_id: &str) -> Result<Vec<ThumbLink>>` — `q = "'<id>' in parents and trashed = false"`, campos `id,thumbnailLink`, páginas de 1000.
- `DriveClient::fetch_thumbnail(&self, link: &str, size: u32) -> Result<Bytes>` — troca o sufixo `=s<N>` (ou acrescenta, se não houver) por `=s{size}`; recusa host fora da regra com `DriveError::Protocol`; pedido autenticado (`send_once`, com recuo).
- `fn image_host_allowed(url: &Url, api_base: &Url) -> bool` (privada), usada por `fetch_photo` e `fetch_thumbnail`.
- No simulador: arquivos com conteúdo e `mime` começando por `image/` ganham `"thumbnailLink": "<origem>/thumb/<id>=s220"`; rota `GET /thumb/{spec}` atrás do mesmo middleware de autenticação das rotas da API, devolvendo um PNG 600×400 (gerado uma vez) e contando em `requests("/thumb")`. `FakeDrive::add_file_with_mime(parent, name, mime, bytes) -> String` para os testes criarem imagens.

- [ ] **Passo 1: testes que falham**

```rust
#[tokio::test]
async fn thumbnail_links_lists_only_files_with_thumbnail() {
    // pasta com foto.jpg (image/jpeg), nota.txt (text/plain), sub/ (pasta)
    // -> exatamente [ThumbLink { id: foto, link: "<origem>/thumb/<foto>=s220" }]
}

#[tokio::test]
async fn fetch_thumbnail_asks_requested_size() {
    // fetch_thumbnail(link, 512) -> bytes decodificam como PNG; o simulador registrou o caminho "/thumb/<id>=s512"
}

#[tokio::test]
async fn fetch_thumbnail_refuses_foreign_host() {
    // "https://exemplo.com/x=s220" -> Err(DriveError::Protocol(_)); "http://lh3.googleusercontent.com/x" -> Err (sem https)
}

#[test]
fn image_host_rule() {
    // aceita https://lh3.googleusercontent.com/..., https://drive.google.com/..., a origem da API;
    // recusa https://googleusercontent.com.evil.com/, https://evil.com/?a=.google.com, http://lh3.googleusercontent.com/
}
```

- [ ] **Passo 2:** `cargo test -p gdl-drive thumbnail` — falha.
- [ ] **Passo 3: implementar** conforme as interfaces; `fetch_photo` passa a usar `image_host_allowed` (comportamento igual, testes dele continuam passando).
- [ ] **Passo 4:** `cargo test -p gdl-drive` e `cargo test -p gdl-fake-drive` passam.
- [ ] **Passo 5: commit** — `Miniaturas: links do Google na API e no Drive simulado`.

### Tarefa 3: Memória do que foi semeado

**Arquivos:**
- Modificar: `crates/gdl-store/src/schema.rs` (migração nova no fim de `MIGRATIONS`)
- Modificar: `crates/gdl-store/src/types.rs` (`ThumbRec`), `crates/gdl-store/src/lib.rs` (métodos)

**Interfaces — produz:**
- Migração: `CREATE TABLE thumbs (item_id TEXT PRIMARY KEY, revision TEXT, modified_ms INTEGER NOT NULL, uri TEXT NOT NULL) WITHOUT ROWID;`
- `pub struct ThumbRec { pub id: ItemId, pub revision: Option<String>, pub modified_ms: i64, pub uri: String }`
- `Store::thumb(&self, id: &ItemId) -> Result<Option<ThumbRec>>`
- `Store::put_thumb(&self, rec: &ThumbRec) -> Result<()>` (upsert)
- `Store::all_thumbs(&self) -> Result<Vec<ThumbRec>>`
- `Store::clear_thumbs(&self) -> Result<()>`
- `Store::is_complete(&self, id: &ItemId, size: u64) -> Result<bool>` = `ranges_missing(id, 0, size)` vazio (reaproveitar; se já existir algo equivalente no `BlockCache`, usar o dele e não criar este).

- [ ] **Passo 1: testes que falham** — `thumbs_roundtrip_and_upsert` (put, put de novo com revisão nova, `thumb` devolve a nova, `all_thumbs` tem 1), `clear_thumbs_empties_table`, e o teste de migração existente estendido: banco na versão anterior abre e ganha a tabela sem perder itens.
- [ ] **Passo 2:** `cargo test -p gdl-store thumbs` — falha.
- [ ] **Passo 3: implementar.**
- [ ] **Passo 4:** `cargo test -p gdl-store` passa.
- [ ] **Passo 5: commit** — `Miniaturas: memória do que foi semeado`.

### Tarefa 4: Semeadura no motor

**Arquivos:**
- Criar: `crates/gdl-engine/src/thumbs/seeder.rs`
- Modificar: `crates/gdl-engine/src/engine.rs` (campos e métodos públicos)

**Interfaces:**
- Consome: Tarefas 1–3.
- Produz:
  - `Engine::seed_thumbnails(self: &Arc<Self>, folder: &ItemId, folder_path: PathBuf) -> tokio::task::JoinHandle<()>` — dispara a semeadura da pasta (não recursiva) e devolve o handle; se a mesma pasta já está em andamento, devolve um handle que termina quando aquela terminar (ou um handle já pronto — o importante é não duplicar downloads). Erros viram `tracing::debug!`, nunca sobem.
  - `Engine::purge_thumbnails(&self)` — cancela semeaduras em andamento, apaga os PNGs de cada `ThumbRec` (`remove_all`) e limpa a tabela.
  - `pub(crate) fn needs_seed(item: &Item, rec: Option<&ThumbRec>, uri: &str, complete: bool) -> bool` — `true` só se: arquivo comum (não pasta, não documento do Google, não atalho), `local_state == Clean`, `!complete`, e `rec` ausente ou com `revision`/`modified_ms`/`uri` diferentes.
  - Campos novos do `Engine`: `thumbs_dir: PathBuf` (de `paths.thumbnails_dir` no `open`), `thumbs_cancel: CancellationToken`, `thumbs_running: Mutex<HashMap<ItemId, Shared<…>>>` ou equivalente.
- Fluxo de `seed_thumbnails`: filhos da pasta pelo banco → filtra com `needs_seed` → se nenhum, termina **sem chamar a API** → `thumbnail_links(folder)` → para cada item que precisa e tem link, até 8 em paralelo: `fetch_thumbnail(link, 512)`, `stored_mtime` do `normal`; se já é `modified_ms/1000`, só grava o `ThumbRec`; senão `write_all` em `spawn_blocking` e depois `put_thumb`. Tudo sob `thumbs_cancel`.
- `Thumb::MTime`: `item.modified_ms.div_euclid(1000)` — a mesma conta de `time_of` no `gdl-fs` (que trunca para segundos no `st_mtime`); expor `pub fn mtime_secs(modified_ms: i64) -> i64` em `gdl_engine::thumbs` e usá-la nos dois lugares para não divergir.

- [ ] **Passo 1: testes que falham** (no `seeder.rs`, com o `FakeDrive` e o `testutil` do motor)

```rust
#[test]
fn needs_seed_rules() {
    // limpo, sem rec, incompleto -> true
    // completo -> false; Dirty/Uploading/Conflict/LocalOnly -> false; pasta/gdoc/atalho -> false
    // rec igual (revisão, modified_ms, uri) -> false; rec com modified_ms diferente -> true; uri diferente -> true
}

#[tokio::test]
async fn seeds_folder_without_downloading_content() {
    // pasta com a.jpg e b.png (image/*) e c.txt; seed_thumbnails(...).await
    // -> 3 PNGs (normal/large/x-large) para a.jpg e b.png em thumbs_dir, com URI de folder_path/<nome>
    // -> fake.downloaded_bytes() == 0; fake.requests("/thumb") == 2; c.txt sem PNG
}

#[tokio::test]
async fn second_seed_makes_no_requests() {
    // depois do primeiro: requests("/drive/v3/files") e requests("/thumb") não mudam no segundo seed
}

#[tokio::test]
async fn remote_change_reseeds_with_new_mtime() {
    // modify_remote(a.jpg) + poll de mudanças; seed de novo -> Thumb::MTime novo e 1 pedido /thumb a mais
}

#[tokio::test]
async fn folder_with_only_docs_does_not_call_api() {
    // pasta com 1 gdoc e 1 subpasta -> requests("/drive/v3/files") igual ao de antes do seed
}

#[tokio::test]
async fn concurrent_seeds_of_same_folder_fetch_once() {
    // dois seed_thumbnails da mesma pasta ao mesmo tempo -> requests("/thumb") == número de imagens
}

#[tokio::test]
async fn api_error_is_swallowed() {
    // fake.inject(Fault de erro 500 na listagem) -> o handle termina sem pânico; nenhum PNG; nada no banco
}

#[tokio::test]
async fn purge_removes_files_and_memory() {
    // depois de semear: purge_thumbnails() -> nenhum PNG em thumbs_dir; all_thumbs() vazio
}
```

- [ ] **Passo 2:** `cargo test -p gdl-engine thumbs` — falha.
- [ ] **Passo 3: implementar** (`tokio::task::JoinSet` + `Semaphore::new(8)`; `CancellationToken` filho de `thumbs_cancel` por semeadura).
- [ ] **Passo 4:** `cargo test -p gdl-engine` passa.
- [ ] **Passo 5: commit** — `Miniaturas: semeadura das pastas no motor`.

### Tarefa 5: `opendir` espera a semeadura

**Arquivos:**
- Modificar: `crates/gdl-fs/src/fs.rs` (campo `mount_point`, `opendir`, `time_of` usando `mtime_secs` para o segundo)
- Modificar: `crates/gdl-fs/src/testutil.rs` (passa `mnt` ao `DriveFs`)
- Testes: `crates/gdl-fs/src/read_tests.rs`

**Interfaces:**
- Consome: `Engine::seed_thumbnails` (Tarefa 4), `Store::path_of`.
- Produz: `DriveFs::new(engine, i18n, uid, gid, mount_point: PathBuf)` (parâmetro novo; ajustar os chamadores) e `const THUMB_WAIT: Duration = Duration::from_millis(1500)`.
- No `opendir`, só para `Node::Item(inode)` de pasta com `path_of` = `Some(rel)`: `folder_path = mount_point / layout.my_drive / rel` (raiz do Meu Drive quando `rel` é vazio); `handle = engine.seed_thumbnails(...)` chamado dentro do `rt`; `rt.block_on(tokio::time::timeout(THUMB_WAIT, handle))`, ignorando o resultado. Montar a listagem como hoje.

- [ ] **Passo 1: testes que falham** (contra a montagem real do `Mounted`)

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn opening_folder_seeds_thumbnails_with_mount_uri_and_mtime() {
    // Meu Drive/Fotos de Março #1/foto (1).jpg (image/jpeg)
    // std::fs::read_dir(drive().join("Fotos de Março #1")) em spawn_blocking
    // -> existe file_for(thumbs_dir, Normal, uri_of(<caminho na montagem>)), com
    //    stored_mtime == metadata(<caminho>).mtime() (o st_mtime real do FUSE)
    // -> fake.downloaded_bytes() == 0
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_thumbnails_do_not_hold_listing_past_deadline() {
    // fake com atraso de 5 s em /thumb (Fault de latência existente, ou um novo se não houver)
    // read_dir termina em < 2,5 s e lista o arquivo; depois o PNG aparece (wait_until de testutil)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn other_fuse_ops_are_not_blocked_while_seeding() {
    // com o atraso de /thumb: durante o read_dir da pasta de fotos, stat de outro arquivo responde em < 200 ms
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trash_and_mount_root_do_not_seed() {
    // read_dir da raiz da montagem e da Lixeira -> requests("/thumb") == 0
}
```

- [ ] **Passo 2:** `cargo test -p gdl-fs thumbnails` — falha.
- [ ] **Passo 3: implementar.**
- [ ] **Passo 4:** `cargo test -p gdl-fs` passa (incluindo os testes de contagem de listagens, que não podem mudar).
- [ ] **Passo 5: commit** — `Miniaturas: a pasta espera a semeadura ao abrir`.

### Tarefa 6: Daemon, saída da conta, versão e documentação

**Arquivos:**
- Modificar: `crates/gdl-daemon/src/daemon.rs` (passar `mount_point.clone()` ao `DriveFs::new`; em `sign_out`, chamar `engine.purge_thumbnails()` antes de `stop_account`)
- Testes: `crates/gdl-daemon/src/dbus_tests.rs`
- Modificar: `Cargo.toml` (versão 0.3.0), `CHANGELOG.md` ([0.3.0] — Adicionado: miniaturas do Google sem baixar), `README.md` (uma linha nos recursos), `docs/testes/README.md` (item do checklist manual: abrir no Dolphin uma pasta de fotos da conta real e conferir que o cache não cresce)

- [ ] **Passo 1: teste que falha** — `logout_removes_seeded_thumbnails`: conta de teste, semeia uma pasta com 1 imagem, `logout("discard", "")` → o diretório de miniaturas do teste não tem mais os PNGs daquele arquivo (e PNGs de outros caminhos colocados no teste continuam lá).
- [ ] **Passo 2:** `cargo test -p gdl-daemon logout_removes` — falha.
- [ ] **Passo 3: implementar.**
- [ ] **Passo 4:** rodar a skill `simplify` no diff da funcionalidade inteira e aplicar o que ela apontar (registrar o que for pulado).
- [ ] **Passo 5:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` e `./scripts/e2e.sh` — tudo passa. (Mudança de cache/armazenamento local e de dependências: rodar também a skill `testes-suite-padrao`.)
- [ ] **Passo 6: verificação real** — `scripts/install-dev.sh`, reiniciar o serviço, abrir no Dolphin **offscreen** (`QT_QPA_PLATFORM=offscreen`, `XDG_CACHE_HOME` do usuário real, como no protótipo) uma pasta de fotos ainda não aberta; conferir no `metadata.db` que `ranges` não cresceu para os .jpg e que os PNGs existem. Nunca abrir janela na tela do usuário.
- [ ] **Passo 7: commit e tag** — `Versão 0.3.0: miniaturas do Google sem baixar os arquivos`, tag `v0.3.0` (sem push).
