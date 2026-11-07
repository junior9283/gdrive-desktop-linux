# Atualização automática — plano de implementação

> **Para quem executa:** SUB-SKILL OBRIGATÓRIA: superpowers:executing-plans (memória
> `sem-subagente-implementador`: as tarefas são feitas na sessão principal). Passos com `- [ ]`.

**Objetivo:** o app oferece "Atualizar agora / Depois" quando há release novo e, ao aceitar, baixa,
confere a assinatura, instala com a senha do polkit e volta a rodar na versão nova.

**Arquitetura:** o serviço (`gdl-daemon`) concentra tudo: consulta, oferta, download, conferência,
execução do instalador (atrás do trait `Installer`) e a troca de programa (`Exit::Upgrade` →
`exec`). A bandeja e a janela só mostram a oferta (pelo D-Bus) e se reexecutam quando o serviço
volta com outra versão.

**Tecnologias:** Rust (tokio, zbus, reqwest via `gdl-drive`, `sha2`, `minisign-verify`), nfpm, JS
da janela (Tauri), GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-07-atualizacao-automatica-design.md`

## Restrições globais

- Comentários, mensagens e commits em pt-BR; `cargo fmt --all` antes de cada commit (o CI exige).
- Textos novos em `locales/pt-BR/*.ftl` e `locales/en/*.ftl`.
- Senha só pela janela do polkit (`pkexec`); nunca `sudo`, nunca senha no terminal ou guardada.
- Nunca instalar sem assinatura minisign do `SHA256SUMS` conferida e SHA-256 do pacote conferido.
- Nunca trocar de variante: o nome do pacote baixado começa com `brand::PACKAGE`.
- Nenhum teste chama `pkexec`, `apt-get`, `dnf`, `zypper` ou `pacman` de verdade.
- Segredos (chave minisign secreta) nunca no repositório nem em log.
- Antes dos testes de cada tarefa: skill `simplify` sobre o código alterado (CLAUDE.md global).
- Cadência: durante a tarefa, só os testes do que mudou; antes de cada commit, `cargo clippy` e os
  testes do crate. Suíte completa (`testes-suite-padrao`) no fim (tarefa 10): a mudança roda
  como root e mexe em dependências e no release.

## Foco da revisão

1. Release cujo `tag_name` é maior, mas falta o pacote da variante/formato em `assets[]` → oferta
   só "Ver novidades", sem erro. Teste em Tarefa 2 (`offer_without_package_cannot_install`).
2. Download interrompido ou servidor devolvendo HTML/404 no lugar do pacote → soma não confere,
   arquivos apagados, aviso de falha, oferta continua. Teste em Tarefa 5
   (`bad_download_fails_safely`).
3. Clique duplo em "Atualizar agora" (notificação + bandeja) → uma atualização só. Teste em
   Tarefa 5 (`second_start_is_ignored`).
4. Serviço reiniciado várias vezes no mesmo dia → no máximo um aviso a cada 24 h. Teste em
   Tarefa 5 (`restart_does_not_repeat_offer`).
5. Senha cancelada no polkit (`pkexec` sai com 126) → sem aviso de erro, volta a "disponível".
   Teste em Tarefa 4 (`pkexec_cancel_is_not_an_error`) e Tarefa 5 (`canceled_password_is_quiet`).

---

### Tarefa 0: o `pkexec` disparado pelo serviço mostra a senha? (manual, bloqueia o resto)

**Arquivos:** Criar `docs/superpowers/decisoes/2026-11-07-pkexec-do-servico.md`

- [ ] **Passo 1:** pedir ao usuário para ficar atento à janela de senha e rodar, fora de qualquer
  sessão de terminal gráfico herdada:
  `systemd-run --user --wait --collect --unit=teste-pkexec pkexec /usr/bin/true; echo $?`
  Esperado: a janela de senha do KDE aparece; digitando, sai `0`; cancelando, `126`.
- [ ] **Passo 2:** registrar o resultado no arquivo de decisão. **Se não abrir janela** (sai
  `127` ou "No authentication agent found"), parar e reescrever as Tarefas 4 e 5 para o caminho
  "a janela executa" da spec (Respostas, item 1) antes de seguir.
- [ ] **Passo 3:** commit `Docs: pkexec a partir do serviço do usuário`.

### Tarefa 1: como o app foi instalado

**Arquivos:**
- Criar: `crates/gdl-daemon/src/upgrade/kind.rs` (módulo novo `upgrade`, com `mod.rs`)
- Modificar: `packaging/nfpm.yaml`, `scripts/package.sh`, `packaging/install.sh`,
  `scripts/install-dev.sh` (opção `--kind`), `crates/gdl-daemon/src/lib.rs`
- Teste: no próprio `kind.rs`; `scripts/stage-manifest.sh`/golden
  `crates/gdl-common/tests/golden/default-stage.sha256` se a lista de arquivos mudar

**Interfaces — produz:**
```rust
pub enum InstallKind { Deb, Rpm, Arch, Tar { prefix: PathBuf }, Source }
/// `exe` canonicalizado → `<prefixo>/bin/<x>` → lê `<prefixo>/share/<PACKAGE>/instalacao`.
pub fn install_kind(exe: &Path) -> InstallKind
pub fn startup_exe() -> PathBuf  // canonicalizado uma vez na partida (OnceLock); usado no exec
```

- [ ] **Passo 1: testes** em `kind.rs` com uma árvore em `tempdir`:
  `reads_marker_next_to_binary` (`deb`→`Deb`, `rpm`→`Rpm`, `arch`→`Arch`, `tar`→`Tar{prefix}`),
  `missing_or_unknown_marker_is_source` (sem arquivo, `fonte`, `xyz` → `Source`),
  `binary_outside_bin_is_source` (`/tmp/x/gdrive-desktop-linuxd` sem `bin/` pai → `Source`).
- [ ] **Passo 2:** `cargo test -p gdl-daemon upgrade::kind` → falha (não existe).
- [ ] **Passo 3:** implementar. Marcador: uma palavra, `trim()`.
- [ ] **Passo 4:** empacotamento: em `nfpm.yaml`, três entradas `contents` para
  `/usr/share/<PACKAGE>/instalacao` com `packager: deb|rpm|archlinux`, cada uma de um arquivo
  gerado por `package.sh` (`deb`, `rpm`, `arch`); `install-dev.sh --kind tar` grava `tar` (e
  entra na lista de desinstalação); `packaging/install.sh` passa `--kind tar`; sem `--kind`, nada
  é gravado (`Source`). A variante GeraSync passa pelo `rename.sed` como os outros caminhos.
- [ ] **Passo 5:** `cargo test -p gdl-daemon upgrade::kind` passa; `scripts/package.sh dist`
  local gera os três pacotes e `dpkg-deb -c`, `rpm -qlp` e `tar -tf` mostram o marcador.
- [ ] **Passo 6:** commit `Atualização: marcador do formato da instalação`.

### Tarefa 2: o release e a oferta

**Arquivos:** Modificar `crates/gdl-daemon/src/updates.rs`, `crates/gdl-fake-drive/src/{lib,state,server}.rs`

**Interfaces — produz:**
```rust
pub struct Release { pub version: Version, pub assets: Vec<Asset> }
pub struct Asset { pub name: String, pub url: Url }
pub async fn latest(url: &Url) -> Result<Option<Release>, HttpError>  // muda o retorno
/// O nome do pacote no release: `<p>_<v>_amd64.deb`, `<p>-<v>-1.x86_64.rpm`,
/// `<p>-<v>-1-x86_64.pkg.tar.zst`, `<p>-<v>-x86_64.tar.gz`; `None` para `Source` ou fora de x86_64.
pub fn package_name(kind: &InstallKind, version: Version) -> Option<String>
pub struct Offer { pub version: Version, pub package: Option<Asset>, pub sums: Option<Asset>, pub signature: Option<Asset> }
impl Offer { pub fn can_install(&self) -> bool }  // os três presentes
pub fn offer(release: Release, kind: &InstallKind) -> Offer
```
Fake: `FakeDrive::set_release(tag: Option<&str>, files: Vec<(String, Vec<u8>)>)` (os arquivos
viram `assets[]` com `browser_download_url` = `<origin>/download/<nome>` e a rota serve os bytes);
`set_latest_release(tag)` continua, como `set_release(tag, vec![])`.

- [ ] **Passo 1: testes:** `package_names_match_package_sh` (os quatro nomes de `1.4.0` para
  `gdrive-desktop-linux`, conferidos com o `ls dist` do release 1.3.2 trocando a versão),
  `offer_with_all_files_can_install`, `offer_without_package_cannot_install` (só `SHA256SUMS` e
  `.minisig` → `can_install() == false`), `offer_ignores_other_flavor` (asset `gerasync_…deb`
  numa instalação `gdrive-desktop-linux` não é escolhido).
- [ ] **Passo 2:** rodar → falha.
- [ ] **Passo 3:** implementar; ajustar `check_release` ao novo retorno (comportamento ainda o
  de hoje) e os testes existentes de `updates.rs`.
- [ ] **Passo 4:** `cargo test -p gdl-daemon updates` e `-p gdl-fake-drive` passam.
- [ ] **Passo 5:** commit `Atualização: arquivos do release e a oferta`.

### Tarefa 3: download e conferência

**Arquivos:**
- Criar: `crates/gdl-daemon/src/upgrade/verify.rs`,
  `crates/gdl-daemon/tests/fixtures/minisign-teste.{pub,key}` (par só de teste, sem senha)
- Modificar: `crates/gdl-drive/src/lib.rs` (`fetch_bytes`), `crates/gdl-common/src/links.rs`
  (`RELEASE_PUBLIC_KEY`), `Cargo.toml`/`crates/gdl-daemon/Cargo.toml` (`minisign-verify`, versão
  atual via `cargo add`)

**Interfaces — produz:**
```rust
pub async fn fetch_bytes(url: &Url) -> Result<Vec<u8>, HttpError>          // gdl-drive
pub const RELEASE_PUBLIC_KEY: &str = "RW…";   // provisória = a de teste até a Tarefa 9
pub enum VerifyError { BadSignature, NotListed, Mismatch }
/// Confere `sig` sobre `sums` com `public_key` e devolve o SHA-256 esperado de `file_name`.
pub fn expected_sha256(sums: &[u8], sig: &[u8], public_key: &str, file_name: &str) -> Result<[u8; 32], VerifyError>
/// Baixa os três arquivos da oferta para `dir` (criado com `create_private_dir`), confere e
/// devolve o caminho do pacote; em qualquer erro apaga `dir`.
pub async fn download(offer: &Offer, dir: &Path, public_key: &str) -> Result<PathBuf, UpgradeError>
pub enum UpgradeError { Network, Verify(VerifyError), Install(String), Busy }
```
`UpdateCheck` ganha `public_key: String` (padrão `RELEASE_PUBLIC_KEY`; o `testutil` põe a de teste).
Pasta do download: `<cache_dir>/atualizacao`, com um campo novo `cache_dir` em `Paths`
(`$XDG_CACHE_HOME/<PACKAGE>`; `for_test` → `root/cache/<PACKAGE>`).

- [ ] **Passo 1: testes** (`verify.rs`, assinando na hora com a chave de teste ou fixtures
  geradas com `minisign -S`): `good_signature_returns_hash`, `tampered_sums_is_bad_signature`
  (um byte trocado), `other_key_is_bad_signature`, `missing_line_is_not_listed`,
  `download_checks_hash_and_cleans_up` (fake com pacote de bytes errados → `Mismatch` e `dir` não
  existe mais).
- [ ] **Passo 2:** rodar → falha.
- [ ] **Passo 3:** implementar. Linhas do `SHA256SUMS`: `<hex>  <nome>`; nome comparado exato.
- [ ] **Passo 4:** testes passam.
- [ ] **Passo 5:** commit `Atualização: download e assinatura do release`.

### Tarefa 4: o instalador

**Arquivos:** Criar `crates/gdl-daemon/src/upgrade/install.rs`

**Interfaces — produz:**
```rust
pub enum InstallOutcome { Done, Canceled }
#[async_trait] pub trait Installer: Send + Sync {
    async fn install(&self, kind: &InstallKind, package: &Path) -> Result<InstallOutcome, UpgradeError>;
}
pub struct SystemInstaller;              // o de verdade
/// A linha de comando de cada formato (pura, para testar).
pub fn command_for(kind: &InstallKind, package: &Path, has: impl Fn(&str) -> bool) -> Option<Vec<OsString>>
```
`Config` ganha `installer: Arc<dyn Installer>` (padrão `SystemInstaller`; o `testutil` usa um
falso que registra a chamada e devolve o resultado configurado).

- [ ] **Passo 1: testes de `command_for`:** `deb` → `pkexec apt-get install -y <abs>`;
  `rpm` com `dnf` → `pkexec dnf install -y <abs>`; `rpm` só com `zypper` →
  `pkexec zypper --non-interactive install --allow-unsigned-rpm <abs>`; `rpm` sem nenhum → `None`;
  `arch` → `pkexec pacman -U --noconfirm <abs>`; `Tar{prefix}` → `sh <extraído>/install.sh
  --prefix <prefix>` (o `.tar.gz` é extraído antes, na mesma pasta privada); `Source` → `None`.
  `pkexec_cancel_is_not_an_error`: status 126 ou 127 do `pkexec` → `Canceled`; outro ≠ 0 →
  `Install(<últimas linhas do stderr>)`.
- [ ] **Passo 2:** rodar → falha.
- [ ] **Passo 3:** implementar `SystemInstaller` com `tokio::process::Command` (`has` = procurar
  no `PATH`); stderr vai para o log detalhado.
- [ ] **Passo 4:** testes passam.
- [ ] **Passo 5:** commit `Atualização: comandos de instalação por formato`.

### Tarefa 5: o fluxo no serviço

**Arquivos:** Modificar `crates/gdl-daemon/src/daemon/releases.rs`, `daemon.rs` (ações, campo de
estado), `notify.rs`, `main.rs` (`Exit::Upgrade`), `locales/*/main.ftl`

**Interfaces — produz:**
```rust
pub const START_UPDATE: &str = "start-update";   // notify.rs
pub const LATER: &str = "update-later";           // só fecha
pub fn new_version(version: &str, can_install: bool, i18n: &I18n) -> Notification
    // can_install: botão START_UPDATE ("Atualizar agora") + extra LATER ("Depois");
    // senão OPEN_RELEASE ("Ver novidades") como hoje
pub fn update_progress(step: UpdateStep, version: &str, i18n: &I18n) -> Notification  // replaces_key "new-version"
pub fn update_failed(reason: &str, i18n: &I18n) -> Notification
pub fn updated(version: &str, i18n: &I18n) -> Notification  // com OPEN_RELEASE
pub enum UpdateStep { Available, Downloading, WaitingPassword, Installing, Failed }
impl Daemon {
    pub async fn check_release(&self);                 // grava a oferta; avisa se a última foi há ≥ 24 h
    pub async fn start_update(self: &Arc<Self>) -> Result<(), ServiceError>;  // Busy se já rodando
    pub fn update_state(&self) -> (Option<Offer>, UpdateStep);
}
pub enum Exit { Quit, Restart, Upgrade }
```
Estado em `<state_dir>/atualizacao.json`: `{ "version": "X.Y.Z", "offered_ms": i64,
"installing": Option<"X.Y.Z"> }`; lê `versao-avisada` antigo como `version` com `offered_ms = 0`
e o apaga. Ao instalar com sucesso: grava `installing`, apaga a pasta baixada, pede
`Exit::Upgrade`; `main.rs` faz o `shutdown` normal e `exec` de `startup_exe()` com os mesmos
argumentos (se o `exec` falhar, loga e segue como `Restart`). Na partida, `installing ==
installed()` → aviso `updated` e limpa `installing`; diferente → aviso `update_failed`.
Toda mudança de `UpdateStep` emite o sinal `update_changed` (Tarefa 6 o cria; aqui um
`watch::Sender` interno).

- [ ] **Passo 1: testes** (em `updates.rs`, com `Harness`, release falso assinado pela chave de
  teste e instalador falso):
  - `offer_has_update_now_and_later` (botões `start-update` e `update-later`);
  - `restart_does_not_repeat_offer` (check, `restart`, check → 1 aviso);
  - `offer_repeats_after_a_day` (`offered_ms` 25 h atrás → aviso de novo);
  - `later_only_dismisses` (ação `update-later` → nenhum download, oferta continua);
  - `update_installs_and_upgrades` (ação `start-update` → instalador chamado com o caminho do
    pacote certo, `atualizacao.json` com `installing`, exit `Upgrade`);
  - `updated_note_after_upgrade` (estado com `installing` = versão atual, partida → aviso
    "Atualizado para …");
  - `bad_download_fails_safely` (pacote adulterado → aviso de falha, instalador não chamado,
    pasta apagada, `UpdateStep::Available`);
  - `canceled_password_is_quiet` (instalador → `Canceled` → sem aviso novo, `Available`);
  - `second_start_is_ignored` (duas chamadas concorrentes → uma instalação);
  - `source_install_opens_release_page` (kind `Source` → `new_version` com "Ver novidades";
    `start_update` abre a página em `h.opened()`).
  Remover `newer_release_is_announced_once` (substituído).
- [ ] **Passo 2:** rodar → falham.
- [ ] **Passo 3:** implementar. `InstallKind` vem de `install_kind(&startup_exe())`, injetável
  pela `Config` (`install_kind: InstallKind`) para os testes.
- [ ] **Passo 4:** textos pt-BR/en: `notify-update-now` "Atualizar agora", `notify-update-later`
  "Depois", `notify-update-downloading` "Baixando a versão { $version }…",
  `notify-update-password` "Digite a senha para instalar a versão { $version }",
  `notify-update-installing` "Instalando a versão { $version }…", `notify-update-failed-title`
  "Não deu para atualizar", `notify-update-failed-network` / `-verify` / `-install` (motivo),
  `notify-updated-title` "Atualizado para a versão { $version }".
- [ ] **Passo 5:** `cargo test -p gdl-daemon` passa.
- [ ] **Passo 6:** commit `Atualização: oferta, Depois e Atualizar agora no serviço`.

### Tarefa 6: D-Bus

**Arquivos:** Modificar `crates/gdl-ipc/src/lib.rs`, `crates/gdl-ipc/src/testing.rs`,
`crates/gdl-daemon/src/dbus.rs`, `crates/gdl-daemon/src/dbus_tests.rs`, `crates/gdl-ui/src/bridge.rs`

**Interfaces — produz:**
```rust
pub struct UpdateInfo {
    pub version: String,      // "" sem oferta
    pub can_install: bool,
    pub step: String,         // available | downloading | waiting-password | installing | failed
}
fn update_info(&self) -> Result<UpdateInfo, ServiceError>;
fn start_update(&self) -> Result<(), ServiceError>;
fn open_release(&self) -> Result<(), ServiceError>;
#[zbus(signal)] fn update_changed(&self) -> zbus::Result<()>;
// Status ganha: pub service_version: String  (CARGO_PKG_VERSION do serviço)
```
Bridge da janela: `update_info`, `start_update`, `open_release` e o sinal `update_changed`
repassado como os outros.

- [ ] **Passo 1: testes** em `dbus_tests.rs`: `update_info_reports_offer`,
  `start_update_over_dbus_runs_installer`, `update_changed_signal_on_step_change`,
  `status_carries_service_version`.
- [ ] **Passo 2–4:** falha → implementar → passa (`cargo test -p gdl-daemon dbus`, `-p gdl-ipc`).
- [ ] **Passo 5:** commit `Atualização: chamadas e sinal no D-Bus`.

### Tarefa 7: bandeja

**Arquivos:** Modificar `crates/gdl-tray/src/model.rs`, `crates/gdl-tray/src/main.rs`,
`locales/*/main.ftl`

**Interfaces:** `Action::StartUpdate`; `TrayView::from_status(status, verbose, update:
&UpdateInfo, i18n)`; item `tray-update` "Atualizar para { $version }" no topo dos dois menus
quando `version != ""` (com `step` ≠ `available`, rótulo do passo e desabilitado). Na bandeja,
`update_changed` chama `refresh`; se `status.service_version != CARGO_PKG_VERSION` e não vazio,
a bandeja faz `exec` de `startup_exe()` (mover `startup_exe` para `gdl-common` se a bandeja não
depende de `gdl-daemon`).

- [ ] **Passo 1: testes** em `model.rs`: `update_item_on_top_when_offered`,
  `no_update_item_without_offer`, `update_item_in_signed_out_menu`,
  `update_item_disabled_while_installing`.
- [ ] **Passo 2–4:** falha → implementar → passa (`cargo test -p gdl-tray`).
- [ ] **Passo 5:** commit `Atualização: item na bandeja`.

### Tarefa 8: janela (tela Sobre)

**Arquivos:** Modificar `crates/gdl-ui/ui/screens/about.js`, `about.css`, `crates/gdl-ui/ui/api.js`,
`locales/*/ui.ftl`, `crates/gdl-ui/src/main.rs` (reexec), testes em `crates/gdl-ui/ui/tests/`

**Interfaces:** `api.updateInfo()`, `api.startUpdate()`, `api.openRelease()`; faixa
`.about-update` logo abaixo de `.about-version`: "Versão { $version } disponível" + botão
preenchido "Atualizar agora" (`can_install`) e botão texto "Ver novidades"; durante um passo,
o texto do passo e sem botão; re-renderiza no sinal `update_changed`. Reexec da janela: no
reconectar ao serviço, se `service_version` ≠ a dela, `exec` do próprio programa com `about`.

- [ ] **Passo 1: testes** (o runner JS do projeto em `ui/tests/`): `about_shows_update_banner`,
  `about_hides_banner_without_offer`, `about_banner_without_install_shows_only_release_link`,
  `about_banner_shows_step_while_installing`.
- [ ] **Passo 2–4:** falha → implementar → passa.
- [ ] **Passo 5:** capturar `docs/imagens/sobre.png` com a faixa (memória `push-atualiza-pagina`).
- [ ] **Passo 6:** commit `Atualização: faixa na tela Sobre`.

### Tarefa 9: assinatura no release (CI) e a chave real

**Arquivos:** Modificar `.github/workflows/release.yml`, `crates/gdl-common/src/links.rs`,
`README.md`

- [ ] **Passo 1:** no job `publish`, depois de baixar `dist`: instalar `minisign` e assinar
  `dist/SHA256SUMS` com o segredo `MINISIGN_SECRET_KEY` (arquivo temporário com `umask 077`,
  apagado no fim) e `MINISIGN_PASSWORD`, gerando `dist/SHA256SUMS.minisig`; falhar se o segredo
  faltar (como `scripts/check-gerasync-secrets.sh`).
- [ ] **Passo 2 (usuário):** gerar o par (`minisign -G -p minisign.pub -s ~/.minisign/gerasync.key`),
  rodar `gh secret set MINISIGN_SECRET_KEY < ~/.minisign/gerasync.key` e
  `gh secret set MINISIGN_PASSWORD`; guardar a chave secreta fora do repositório.
- [ ] **Passo 3:** pôr a pública em `RELEASE_PUBLIC_KEY` e na seção "Como instalar" do README
  (como conferir: `minisign -Vm SHA256SUMS -P <chave>`).
- [ ] **Passo 4:** `workflow_dispatch` do Release (só gera e testa) passa; o passo de assinatura
  roda numa tag de teste ou é conferido localmente com a mesma linha de comando.
- [ ] **Passo 5:** commit `Release: SHA256SUMS assinado com minisign`.

### Tarefa 10: fechamento

- [ ] **Passo 1:** versão 1.4.0 no `Cargo.toml`; CHANGELOG (incluindo "quem está no 1.3.2 ou
  antes instala esta à mão uma vez"); README (tabela de recursos) e `docs/index.md`.
- [ ] **Passo 2:** suíte completa (`testes-suite-padrao`): `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets`, `cargo test --workspace`, testes JS da janela.
- [ ] **Passo 3: teste de ponta a ponta manual nesta máquina (Nobara, `.rpm`):** instalar o
  `.rpm` 1.4.0 gerado localmente; apontar `UpdateCheck.url` para um release falso servido em
  `localhost` com um "1.4.1" assinado pela chave real (variável de ambiente só de
  desenvolvimento, ex. `GDL_RELEASES_URL`; não é brecha, porque a assinatura continua conferida
com a chave embutida); clicar em
  "Atualizar agora", digitar a senha e conferir: serviço, bandeja e janela na 1.4.1, aviso
  "Atualizado". Repetir cancelando a senha.
- [ ] **Passo 4:** commit `Versão 1.4.0`. Push e tag só com pedido explícito do usuário.
