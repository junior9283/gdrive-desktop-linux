# GeraSync (variante verificada) — Plano de implementação

> **Para quem executa:** SUB-SKILL OBRIGATÓRIA: superpowers:executing-plans (neste projeto as tarefas
> são feitas por mim, sem subagente implementador). Os passos usam checkbox (`- [ ]`).

**Objetivo:** gerar, do mesmo código, uma segunda variante do app (**GeraSync**) com Client ID oficial
embutido e identidade própria. A variante atual continua idêntica à de hoje. Junto vêm o site e o
material do pedido de verificação ao Google.

**Arquitetura:**
- **Rust:** a identidade vem do módulo `gdl_common::brand`, escolhido na compilação pelo `--cfg
  gerasync` (em `RUSTFLAGS`).
- **Arquivos de empacotamento e integrações:** os da variante atual não mudam. Os do GeraSync são
  derivados deles na instalação por uma tabela de substituição (`packaging/gerasync/rename.sed`).
- **Textos:** o GeraSync carrega por cima um `.ftl` próprio, só com as mensagens que dão nome ao app.
- **Tauri:** recebe `identifier` e `productName` pelo `TAURI_CONFIG`.

**Tecnologias:** Rust (workspace atual, zbus, fluent, Tauri 2), sh, sed, nfpm, GitHub Actions, HTML
estático (GitHub Pages).

**Desenho:** `docs/superpowers/specs/2026-11-04-gerasync-variante-verificada-design.md`

## Restrições globais

- **Variante atual:** os arquivos que o pacote da variante atual instala ficam byte a byte
  iguais aos de hoje (teste dourado da Tarefa 1). Os binários da variante atual mantêm os mesmos
  nomes, caminhos e IDs.
- **Nomes e IDs do GeraSync:**

  | Item | Valor |
  |---|---|
  | Nome exibido | `GeraSync` |
  | ID do app | `io.github.junior9283.GeraSync` |
  | D-Bus | `io.github.junior9283.GeraSync1` (caminho `/io/github/junior9283/GeraSync1`) |
  | Pacote, pastas XDG e chaveiro | `gerasync` |
  | Daemon | `gerasyncd` |
  | Pasta montada | `~/GeraSync` |
  | Site | `https://junior9283.github.io/gerasync/` |
  | E-mail | `gerasync@googlegroups.com` |
  | Nome na tela do Google | `GeraSync for Google Drive` |

- **Repositório e releases:** continuam `https://github.com/junior9283/gdrive-desktop-linux` nas
  duas variantes (issues, releases, atualização). Esse endereço **não** pode virar `.../gerasync`
  na substituição.
- **Proibido no GeraSync:** nenhum arquivo, caminho, mensagem ou binário pode conter
  `GDriveDesktopLinux`, `gdrive-desktop-linux` ou `gdrive_desktop_linux`. A única exceção é o
  endereço do repositório acima.
- **Credenciais:** `GERASYNC_CLIENT_ID` e `GERASYNC_CLIENT_SECRET` vêm do ambiente na compilação.
  Sem elas, a compilação com `--cfg gerasync` falha, a menos que exista
  `GERASYNC_ALLOW_FAKE_CREDS=1`. Nesse caso o build usa `gerasync-fake.apps.googleusercontent.com`
  e `fake-secret`. Os valores nunca vão para arquivo commitado nem para log.
- **Escopos:** os mesmos de hoje: `openid email https://www.googleapis.com/auth/drive`.
- **Textos da interface:** "Google Drive" continua quando fala do serviço. Passa a "GeraSync"
  quando dá nome ao app (desenho, decisão 4).
- **`RUSTFLAGS`:** o `--cfg gerasync` é **acrescentado** ao `RUSTFLAGS` existente, porque o
  `setup-rust-toolchain` do CI já põe `-D warnings` nele. O build do GeraSync usa
  `CARGO_TARGET_DIR=target/gerasync`.
- **Cadência de cada tarefa:** skill `simplify` sobre o que mudou, depois os testes da tarefa,
  depois `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` e commit em
  pt-BR. Push só com pedido explícito.

## Pontos de atenção da revisão

1. **Instalar as duas variantes pelo mesmo gerenciador (.deb ou .rpm):** nenhum arquivo em comum,
   para não dar conflito de pacote. O teste de arquivos disjuntos fica na Tarefa 6.
2. **Dados copiados da variante atual para o GeraSync:** um Client ID guardado no chaveiro ou na
   configuração é ignorado. Teste na Tarefa 4 (`resume_uses_official_credentials`).
3. **Release com segredo ausente ou falso:** falha em segundos, antes de compilar. É a checagem
   da Tarefa 8, mais o pânico do `build.rs` da Tarefa 4.
4. **Substituição que acerta o endereço do repositório ou o tipo MIME `application/x-gdrive-*`:**
   os dois precisam continuar iguais. Teste na Tarefa 6 (`gerasync_keeps_repo_url_and_mime_types`).
5. **Os dois serviços rodando juntos:** nomes D-Bus, unidade systemd, pasta montada e pastas XDG
   diferentes. Teste na Tarefa 2 (`brand_values_differ_between_flavors`) e prova manual na Tarefa 10.

---

### Tarefa 1: Teste dourado da variante atual

Trava o que a variante atual instala antes de qualquer mudança.

**Arquivos:**
- Criar: `scripts/stage-manifest.sh`
- Criar: `crates/gdl-common/tests/golden/default-stage.sha256`
- Modificar: `crates/gdl-common/tests/packaging.rs`

**Interfaces:**
- Produz: `scripts/stage-manifest.sh [--flavor default|gerasync] <saída>`. Instala com
  `install-dev.sh --prefix /usr --destdir <tmp> --from <fake bins> --no-dolphin` e imprime uma linha
  `sha256  /caminho` por arquivo instalado, ordenada, com o conteúdo dos binários falsos fora da
  soma.

- [ ] **Passo 1:** criar `scripts/stage-manifest.sh`. Ele aceita `--flavor`, mas por enquanto só o
  `default` funciona. Gerar o dourado com
  `sh scripts/stage-manifest.sh crates/gdl-common/tests/golden/default-stage.sha256` no HEAD
  `1fd54d4`, sem nenhuma outra mudança.
- [ ] **Passo 2:** escrever o teste `default_stage_matches_golden` em `packaging.rs`. Ele roda o
  script num diretório temporário e faz `assert_eq!` com o dourado. Na falha, mostra o diff linha
  a linha.
- [ ] **Passo 3:** `cargo test -p gdl-common --test packaging default_stage_matches_golden`.
  Esperado: PASS.
- [ ] **Passo 4:** commit "Testes: o que a variante atual instala fica travado num arquivo dourado".

### Tarefa 2: Módulo `brand` e o `--cfg gerasync`

**Arquivos:**
- Criar: `crates/gdl-common/src/brand.rs`
- Modificar: `crates/gdl-common/src/lib.rs`. `APP_ID` passa a ser `pub use brand::APP_ID`.
- Modificar: `Cargo.toml` da raiz:
  `[workspace.lints.rust] unexpected_cfgs = { level = "warn", check-cfg = ['cfg(gerasync)'] }`, e
  `lints.workspace = true` em cada crate que ainda não tiver.
- Criar: `scripts/cargo-gerasync.sh`

**Interfaces:**
- Produz, em `gdl_common::brand`, as constantes `&str` abaixo e mais `OFFICIAL: bool`
  (`false`/`true`). Cada valor está no formato "variante atual / GeraSync":

  | Constante | Variante atual | GeraSync |
  |---|---|---|
  | `APP_ID` | `io.github.junior9283.GDriveDesktopLinux` | `io.github.junior9283.GeraSync` |
  | `PACKAGE` | `gdrive-desktop-linux` | `gerasync` |
  | `DAEMON` | `gdrive-desktop-linuxd` | `gerasyncd` |
  | `TRAY_PROGRAM` | `gdrive-desktop-linux-tray` | `gerasync-tray` |
  | `UI_PROGRAM` | `gdrive-desktop-linux-ui` | `gerasync-ui` |
  | `MCP_PROGRAM` | `gdrive-desktop-linux-mcp` | `gerasync-mcp` |
  | `DISPLAY_NAME` | `Google Drive` | `GeraSync` |
  | `MOUNT_DIR` | `Google Drive` | `GeraSync` |
  | `BUS_NAME` | `io.github.junior9283.GDriveDesktopLinux1` | `io.github.junior9283.GeraSync1` |
  | `OBJECT_PATH` | `/io/github/junior9283/GDriveDesktopLinux1` | `/io/github/junior9283/GeraSync1` |
  | `UI_BUS_NAME`, `UI_OBJECT_PATH`, `TRAY_BUS_NAME` | `BUS_NAME` + `.Ui` / `OBJECT_PATH` + `/Ui` / `BUS_NAME` + `.Tray` | idem |
  | `SITE_URL` | `https://junior9283.github.io/gdrive-desktop-linux/` | `https://junior9283.github.io/gerasync/` |

  `PROJECT_URL = "https://github.com/junior9283/gdrive-desktop-linux"` é o mesmo nas duas.
- **Como definir:** um `macro_rules! brand` recebe os literais e define todas as constantes,
  derivando as do D-Bus com `concat!`. Ele é chamado duas vezes, sob `#[cfg(not(gerasync))]` e
  `#[cfg(gerasync)]`.
- Produz `scripts/cargo-gerasync.sh <args do cargo>`. Ele exporta
  `RUSTFLAGS="${RUSTFLAGS:-} --cfg gerasync"`, `CARGO_TARGET_DIR=target/gerasync` e
  `TAURI_CONFIG='{"identifier":"io.github.junior9283.GeraSync","productName":"gerasync"}'`, depois
  roda `cargo "$@"`.

- [ ] **Passo 1:** escrever os testes em `brand.rs`.
  - `default_values_match_v1_2_3`, sob `#[cfg(not(gerasync))]`: compara cada constante com a coluna
    "variante atual".
  - `gerasync_values`, sob `#[cfg(gerasync)]`: compara com a coluna "GeraSync" e garante que
    nenhuma constante, exceto `PROJECT_URL`, contém `gdrive` ou `GDrive`.
  - `brand_values_differ_between_flavors`: garante que `BUS_NAME`, `PACKAGE`, `MOUNT_DIR` e
    `APP_ID` aparecem só uma vez numa tabela fixa com os valores das duas variantes.
- [ ] **Passo 2:** `cargo test -p gdl-common brand`. Esperado: falha de compilação (o módulo ainda
  não existe).
- [ ] **Passo 3:** implementar `brand.rs`, `lib.rs`, os lints e o `cargo-gerasync.sh`.
- [ ] **Passo 4:** rodar `cargo test -p gdl-common brand` e
  `sh scripts/cargo-gerasync.sh test -p gdl-common brand`. Esperado: PASS nos dois.
- [ ] **Passo 5:** commit "Identidade do app num módulo só, com a variante GeraSync no --cfg
  gerasync".

### Tarefa 3: Código Rust lendo a identidade do `brand`

**Arquivos (modificar, trocando os literais pelas constantes):**
- `crates/gdl-common/src/{paths.rs,links.rs,gdocs.rs,config.rs}`. O `DefaultMountPoint` passa a
  usar `brand::MOUNT_DIR`; `links::PROJECT` passa a usar `brand::PROJECT_URL`.
- `crates/gdl-drive/src/{token_store.rs,lib.rs}`. O atributo do chaveiro passa a usar
  `brand::PACKAGE`.
- `crates/gdl-ipc/src/{lib.rs,launcher.rs}`. As constantes viram re-exports do `brand`. Nos
  atributos `#[zbus::proxy(...)]` e `#[zbus(prefix = ...)]`, os literais vão em pares
  `#[cfg_attr(not(gerasync), ...)]` / `#[cfg_attr(gerasync, ...)]`, porque o atributo não aceita
  constante.
- `crates/gdl-daemon/src/{dbus.rs,desktop.rs,mount.rs,menu.rs,cosmic.rs,logging.rs,main.rs,daemon/diagnostics.rs}`.
  O `#[zbus::interface(name = ...)]` usa o mesmo par de `cfg_attr`. O `"Google Drive"` da barra
  lateral e do menu passa a `brand::DISPLAY_NAME`. O `TRAY_EXEC` passa a
  `/usr/bin/{TRAY_PROGRAM}`. O `LOG_FILE` passa a `{DAEMON}.log`.
- `crates/gdl-cli/src/{main.rs,uninstall.rs,menu_action.rs}`,
  `crates/gdl-tray/src/main.rs`, `crates/gdl-ui/src/{main.rs,single.rs,bridge.rs}`,
  `crates/gdl-mcp/src/lib.rs`, `crates/gdl-fs/src/fs.rs` e `crates/gdl-engine/src/gdocs.rs`.
- Os nomes `[[bin]]` nos `Cargo.toml` **não mudam**. A Tarefa 6 renomeia os binários na
  instalação.

**Interfaces:**
- Consome: `gdl_common::brand::*` (Tarefa 2).

- [ ] **Passo 1:** escrever o teste `no_identity_literals_outside_brand` em
  `crates/gdl-common/tests/brand_literals.rs`. Ele percorre `crates/*/src/**/*.rs`, menos
  `brand.rs` e os arquivos de teste. Falha se achar `GDriveDesktopLinux` ou
  `"gdrive-desktop-linux` fora de uma linha `cfg_attr(not(gerasync)`. Também falha se achar
  `"Google Drive"` em `mount.rs`, `desktop.rs`, `menu.rs` ou `menu_action.rs`.
- [ ] **Passo 2:** rodar o teste. Esperado: FAIL, listando os arquivos.
- [ ] **Passo 3:** fazer as trocas.
- [ ] **Passo 4:** rodar `cargo test --workspace` (esperado: PASS, sem mudar nenhum teste
  existente) e `GERASYNC_ALLOW_FAKE_CREDS=1 sh scripts/cargo-gerasync.sh test --workspace`
  (esperado: PASS; os testes do D-Bus já sobem o serviço pelo proxy, o que prova que nome e
  interface batem na variante GeraSync).
- [ ] **Passo 5:** rodar `cargo test -p gdl-common --test packaging`. Esperado: o dourado continua
  passando.
- [ ] **Passo 6:** commit "Código: nomes, pastas e D-Bus vêm da identidade do app".

### Tarefa 4: Credenciais oficiais embutidas

**Arquivos:**
- Criar ou modificar: `crates/gdl-daemon/build.rs`.
- Modificar: `crates/gdl-daemon/src/daemon.rs`, em `credentials_for`, `begin_login` e
  `resume_account`.
- Modificar: `crates/gdl-cli/src/main.rs` (comando `login`).

**Interfaces:**
- Produz `gdl_daemon::official::credentials() -> ClientCredentials`, sob `#[cfg(gerasync)]`. Os
  valores vêm de `env!("GDL_OFFICIAL_CLIENT_ID")` e `env!("GDL_OFFICIAL_CLIENT_SECRET")`, que o
  `build.rs` emite.
- **`build.rs`:** só age se `CARGO_CFG_GERASYNC` estiver definido. Lê `GERASYNC_CLIENT_ID` e
  `GERASYNC_CLIENT_SECRET`. Se faltarem e `GERASYNC_ALLOW_FAKE_CREDS` não for `1`, chama
  `panic!("GeraSync: defina GERASYNC_CLIENT_ID e GERASYNC_CLIENT_SECRET")`, sem imprimir valor
  nenhum. Emite `cargo:rerun-if-env-changed` para as três variáveis.
- No GeraSync, `credentials_for(given, expected)` devolve sempre `official::credentials()` e
  ignora o `given` e o que estiver salvo. `resume_account` também usa as oficiais.
- No GeraSync, o `login` da CLI não aceita `--client-id` nem `--client-secret-file`: os campos
  ficam sob `#[cfg(not(gerasync))]`, e o `login` não pergunta nada.

- [ ] **Passo 1:** escrever os testes em `daemon.rs`, sob `#[cfg(gerasync)]` e com credenciais
  falsas.
  - `login_ignores_given_credentials`: chama `begin_login` com o ID `outro.apps.googleusercontent.com`.
    A URL devolvida contém `client_id=gerasync-fake.apps.googleusercontent.com`.
  - `resume_uses_official_credentials`: com uma conta salva com outro ID, depois de retomar, a
    renovação no `gdl-fake-drive` recebe o ID falso oficial.
- [ ] **Passo 2:** rodar
  `GERASYNC_ALLOW_FAKE_CREDS=1 sh scripts/cargo-gerasync.sh test -p gdl-daemon official`.
  Esperado: FAIL.
- [ ] **Passo 3:** implementar.
- [ ] **Passo 4:** rodar o comando do Passo 2 (esperado: PASS) e
  `sh scripts/cargo-gerasync.sh build -p gdl-daemon` sem as variáveis (esperado: falha com a
  mensagem acima). Depois rodar `cargo test -p gdl-daemon` (esperado: PASS na variante atual).
- [ ] **Passo 5:** commit "GeraSync: login com o Client ID oficial embutido no build".

### Tarefa 5: Textos e interface do GeraSync

**Arquivos:**
- Criar: `locales/pt-BR/gerasync.ftl` e `locales/en/gerasync.ftl`.
- Modificar: `crates/gdl-common/src/i18n.rs`. Sob `cfg(gerasync)`, adiciona o `.ftl` do GeraSync
  com `add_resource_overriding`. Se o idioma não tiver o arquivo, cai no `en`.
- Modificar: `crates/gdl-ui/src/bridge.rs`, com o comando Tauri novo
  `brand() -> BrandInfo { official: bool, name: String, site: String }`.
- Modificar: `crates/gdl-ui/ui/screens/onboarding/model.js`, `onboarding.js` e `about.js`.
- Testes: `crates/gdl-ui/ui/tests/onboarding.model.test.js`.

**Mensagens que o GeraSync sobrescreve**, nos dois idiomas, trocando o nome do app por
"GeraSync":
- **`ui.ftl`:** `about-name`, `quit-title`, `onboarding-files-text`,
  `onboarding-offline-command` (`gerasync pin <caminho>`), `pref-drive-folder`,
  `onboarding-summary-mount`, `folder-refused-nested`, `folder-refused-contains`,
  `mirror-config-text`, `uninstall-title`, `uninstall-body`, `uninstall-done`, `share-missing`,
  `versions-missing`.
- **`main.ftl`:** `login-page-title`, `notify-title`, `notify-offline-body`,
  `notify-disk-low-body`, `cli-uninstall-confirm`, `cli-uninstall-done`, `cli-no-service`,
  `tray-no-service`, `tray-open-folder`, `tray-hotkey-search`, `window-home`, `window-uninstall`.

Ficam iguais (falam do serviço): `search-placeholder`, `link-drive-web`, `tray-open-web`,
`pref-tab-drive`, os `error-category-*` e as mensagens sobre "Computadores no Google Drive". O
`about-unofficial` do GeraSync diz: "GeraSync is an unofficial Google Drive client and is not
affiliated with or endorsed by Google." / "O GeraSync é um cliente não oficial do Google Drive,
sem ligação com o Google nem aprovação dele."

**Interfaces:**
- `model.js`: `initialState({ official })`. Com `official: true`, a lista de passos não tem os de
  Client ID e o passo `sign-in` não valida `clientId`/`secret`.

- [ ] **Passo 1:** escrever os testes.
  - Rust, em `i18n.rs`:
    - `gerasync_overrides_only_known_keys`: toda chave dos `gerasync.ftl` existe no `main.ftl` ou
      no `ui.ftl` do mesmo idioma.
    - `gerasync_messages_never_name_the_old_app` (sob `cfg(gerasync)`): nenhuma mensagem
      renderizada, nos dois idiomas, contém `gdrive-desktop-linux`. Além disso,
      `tr("notify-title") == "GeraSync"`.
  - JS:
    - `official onboarding skips the client steps`: nenhum passo com `id: 'client'`.
    - `official sign-in needs no client id`: o `next()` no `sign-in` com `clientId` vazio não
      gera `onboarding-missing-client`.
- [ ] **Passo 2:** rodar os testes (o Rust pelo `cargo-gerasync.sh test -p gdl-common i18n`, o JS
  pelo `node --test 'crates/gdl-ui/ui/tests/*.test.js'`). Esperado: FAIL.
- [ ] **Passo 3:** implementar os `.ftl`, o `i18n.rs`, o comando `brand` e as telas. O `about.js`
  linka `site` e esconde o link do guia do Client ID quando `official`.
- [ ] **Passo 4:** rodar de novo os testes do Passo 2 e também `cargo test -p gdl-common i18n`.
  Esperado: PASS.
- [ ] **Passo 5:** commit "GeraSync: nome nos textos e primeiro uso sem guia do Client ID".

### Tarefa 6: Instalação e pacotes do GeraSync

**Arquivos:**
- Criar: `packaging/gerasync/rename.sed`.
- Criar: `assets/gerasync/icons/hicolor/<N>x<N>/apps/io.github.junior9283.GeraSync.png` (ícone
  provisório da Tarefa 7).
- Modificar: `scripts/install-dev.sh` (`--flavor default|gerasync`), `scripts/package.sh`
  (`--flavor`), `scripts/stage-manifest.sh`, `integrations/dolphin/build.sh` (`--flavor`) e
  `crates/gdl-common/tests/packaging.rs`.

**`rename.sed`, nesta ordem:**
1. `s|github.com/junior9283/gdrive-desktop-linux|@@REPO@@|g`
2. `s/io\.github\.junior9283\.GDriveDesktopLinux/io.github.junior9283.GeraSync/g`
3. `s|/io/github/junior9283/GDriveDesktopLinux|/io/github/junior9283/GeraSync|g`
4. `s/gdrive-desktop-linux/gerasync/g`
5. `s/gdrive_desktop_linux/gerasync/g`
6. `s/gdl-emblem-/gerasync-emblem-/g`
7. As trocas do nome exibido nos arquivos de empacotamento e integrações. Por exemplo:
   - `Name=Google Drive` → `Name=GeraSync`;
   - `Description=Google Drive para Linux` → `Description=GeraSync`;
   - os rótulos do menu "Google Drive" no Dolphin e no GTK → `GeraSync`;
   - os `Comment` que falam do app.

   O executor lista cada linha alterada no commit.
8. `s|@@REPO@@|github.com/junior9283/gdrive-desktop-linux|g`

**Comportamento com `--flavor gerasync`:**
- **`install-dev.sh`:** passa o conteúdo e o caminho de destino de cada arquivo de texto pelo
  `rename.sed`. Copia `gdrive-desktop-linux*` de `--from` como `gerasync*`. Usa os ícones de
  `assets/gerasync/`. Os emblemas são instalados como `gerasync-emblem-*.svg`. O
  `installed-files` vai para `share/gerasync/`.
- **`package.sh`:** usa o `nfpm.yaml` passado pelo mesmo sed, com `homepage:
  https://junior9283.github.io/gerasync/`. Gera os nomes `gerasync-<versão>…`. O `SHA256SUMS`
  passa a ser **acrescentado**, não sobrescrito, para as duas variantes caberem na mesma `dist/`.
- **`integrations/dolphin/build.sh --flavor gerasync`:** copia `integrations/dolphin` para uma
  pasta temporária, aplica o sed e compila.

- [ ] **Passo 1:** escrever os testes em `packaging.rs`.
  - `gerasync_stage_has_no_old_names`: nenhum caminho ou conteúdo do manifesto do GeraSync contém
    os três nomes proibidos, fora `github.com/junior9283/gdrive-desktop-linux`.
  - `flavors_install_disjoint_files`: a interseção dos caminhos dos dois manifestos é vazia.
  - `gerasync_keeps_repo_url_and_mime_types`: o `.service` do systemd do GeraSync tem
    `Documentation=https://github.com/junior9283/gdrive-desktop-linux`, e o XML de MIME mantém
    `application/x-gdrive-document`.
  - `gerasync_desktop_files_are_valid`: `desktop-file-validate` passa em cada `.desktop`.
  - `default_stage_matches_golden` continua sem mudança.
- [ ] **Passo 2:** `cargo test -p gdl-common --test packaging`. Esperado: FAIL nos novos.
- [ ] **Passo 3:** implementar.
- [ ] **Passo 4:** rodar o teste do Passo 2 (esperado: PASS, inclusive o dourado). Depois rodar
  `sh integrations/dolphin/build.sh --flavor gerasync /tmp/claude-1000/dolphin-gs` e
  `integrations/gtk/tests/run.sh` (esperado: PASS).
- [ ] **Passo 5:** commit "Pacotes do GeraSync, lado a lado com os da variante atual".

### Tarefa 7: Ícone do GeraSync

**Arquivos:**
- Criar: `assets/gerasync/app-icon.svg`.
- Modificar: `scripts/update-app-icon.py`, que ganha
  `--flavor gerasync --source <svg>` e gera por `rsvg-convert` os PNG em `assets/gerasync/icons/...`
  e os dois ícones da janela e da tela Sobre do GeraSync.
- Ícones da janela e do Sobre: o GeraSync usa `crates/gdl-ui/icons/gerasync.png` (via
  `TAURI_CONFIG`, `bundle.icon`) e `crates/gdl-ui/ui/icons/app-gerasync.png`, escolhido pelo
  `about.js` quando `official`.

- [ ] **Passo 1:** gerar o ícone provisório, um quadrado com "GS" e a cor `#2b6cb0`, nos tamanhos
  `16 22 24 32 48 64 128 256 512`. Rodar `cargo test -p gdl-common --test packaging`. Esperado:
  PASS.
- [ ] **Passo 2:** desenhar 3 propostas em SVG (nuvem com setas, sem triângulo e sem as cores do
  Google), abrir uma página de comparação no Chrome e pedir a escolha ao usuário.
- [ ] **Passo 3:** gerar os PNG da escolhida e o logotipo de 120×120 px para o Console
  (`assets/gerasync/logo-120.png`). Rodar o teste do Passo 1. Esperado: PASS.
- [ ] **Passo 4:** commit "GeraSync: ícone próprio".

### Tarefa 8: CI e release com as duas variantes

**Arquivos:**
- Modificar: `.github/workflows/ci.yml`. Novo job `gerasync`, com o mesmo setup do `check`, que
  roda `GERASYNC_ALLOW_FAKE_CREDS=1 sh scripts/cargo-gerasync.sh clippy --workspace --all-targets
  -- -D warnings` e `... test --workspace`. O job `integrations` também compila o Dolphin com
  `--flavor gerasync`.
- Modificar: `.github/workflows/release.yml`:
  - **Primeiro passo do `build`:** `test -n "$GERASYNC_CLIENT_ID" && test -n
    "$GERASYNC_CLIENT_SECRET"`, e nenhum dos dois contém `fake`. Os valores vêm de
    `${{ secrets.GERASYNC_CLIENT_ID }}` e `${{ secrets.GERASYNC_CLIENT_SECRET }}`. Sem
    `GERASYNC_ALLOW_FAKE_CREDS`.
  - **Pacotes:** compila e empacota as duas variantes na mesma `dist/`.
  - **Testes de instalação:** a matriz `install` ganha `flavor: [default, gerasync]` e testa
    `${PROGRAM} --version` e os plugins com os nomes de cada variante.
  - **Notas do release:** ganham uma linha explicando a diferença: "gerasync-*: instale e entre,
    sem criar Client ID; gdrive-desktop-linux-*: use o seu próprio Client ID".

- [ ] **Passo 1:** validar os YAML com `python3 -I -c "import yaml,sys;
  [yaml.safe_load(open(f)) for f in sys.argv[1:]]" .github/workflows/*.yml`. Esperado: sem erro.
- [ ] **Passo 2:** simular o `build` localmente, com os segredos ausentes. A checagem precisa
  falhar. Com `GERASYNC_CLIENT_ID=x.apps.googleusercontent.com GERASYNC_CLIENT_SECRET=y`,
  `scripts/package.sh --flavor gerasync dist-gs` deve gerar os 4 pacotes.
- [ ] **Passo 3:** commit "CI e release: as duas variantes compiladas, testadas e publicadas".
  O workflow só roda de verdade no push, que depende de pedido do usuário.

### Tarefa 9: Material da verificação e site do GeraSync

**Arquivos:**
- Criar: `docs/verificacao-google/checklist.md` (pt-BR). Os passos do Console na ordem do desenho,
  com os valores exatos: nome `GeraSync for Google Drive`, e-mail `gerasync@googlegroups.com`,
  página inicial, `.../privacy.html`, `.../terms.html`, domínio autorizado `junior9283.github.io`,
  os três escopos, cliente "App para computador", segredos `GERASYNC_CLIENT_ID` e
  `GERASYNC_CLIENT_SECRET`.
- Criar: `docs/verificacao-google/scope-justification.md` (en).
- Criar: `docs/verificacao-google/video-script.md` (en). Mostra o consentimento em inglês, o
  `client_id` visível na URL e cada uso do escopo: montar, abrir sob demanda, enviar uma alteração
  local, "Disponível off-line" e backup de uma pasta.
- Criar: `docs/verificacao-google/casa-answer.md` (en).
- Modificar: `docs/_config.yml`, para excluir `verificacao-google` do site atual.
- Criar o site, fora deste repositório, em `/mnt/ssd/Documentos/APP/gerasync/`:
  - `index.html` (en), `pt-br/index.html`, `privacy.html`, `pt-br/privacidade.html`, `terms.html`,
    `pt-br/termos.html`, `style.css` e o logotipo;
  - um `<meta name="google-site-verification">` com o valor que o usuário fornecer;
  - `<title>` e `<h1>` exatamente `GeraSync`;
  - o texto "GeraSync is not affiliated with Google", o e-mail e o link dos releases.

  A política segue a seção Limited Use do Google e não diz substituir revisão jurídica.

- [ ] **Passo 1:** escrever os documentos e o site. Conferir os links com
  `python3 -I -m http.server` na pasta do site e `curl` em cada página. Esperado: 200 e nenhum link
  quebrado.
- [ ] **Passo 2:** mostrar o site no Chrome ao usuário. Com o ok dele, rodar `gh repo create
  junior9283/gerasync --public`, enviar o conteúdo, ligar o Pages pelo `gh api` e conferir
  `https://junior9283.github.io/gerasync/` com `curl`. Esperado: 200.
- [ ] **Passo 3:** commit no repositório do app: "Docs: material do pedido de verificação do
  GeraSync".

### Tarefa 10: Fechamento

- [ ] **Passo 1:** `simplify` sobre a branch inteira. Depois a skill `testes-suite-padrao`
  (mudança estrutural e de auth), nas duas variantes. Depois `scripts/e2e.sh`.
- [ ] **Passo 2:** prova manual de convivência. Instalar as duas em `~/.local` pelo
  `install-dev.sh` (o GeraSync com credenciais falsas só para subir). Os dois serviços sobem
  juntos (`systemctl --user status gdrive-desktop-linuxd gerasyncd`), cada um com a própria pasta
  e o próprio ícone na bandeja. Depois desinstalar o GeraSync pela CLI dele e conferir que a
  variante atual continua de pé.
- [ ] **Passo 3:** atualizar o README, o `docs/index.md` (os dois downloads e a diferença entre
  eles), as capturas que mudaram e o CHANGELOG.
- [ ] **Passo 4:** `cargo fmt --all` e commit "Docs: as duas variantes no README e na página".
  Push só com pedido.

### Tarefa 11: Console do Google, guiado (passos do usuário)

- [ ] **Passo 1:** pelo Chrome, guiar o usuário no `checklist.md`: projeto, Drive API, Branding,
  Search Console (a meta tag vai no site, e eu republico), escopos, cliente desktop e segredos
  no GitHub (`gh secret set` com o valor colado pelo usuário no terminal, nunca no chat).
- [ ] **Passo 2:** depois do primeiro release com o GeraSync, o usuário grava o vídeo pelo roteiro
  e faz os envios (marca, depois escopo). Registrar as respostas do Google em
  `docs/verificacao-google/historico.md`.
