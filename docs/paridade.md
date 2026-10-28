# Matriz de paridade

Situação de cada item do catálogo da [especificação](superpowers/specs/2026-10-25-gdrive-desktop-linux-design.md)
(seção 6) no código, conferida depois da versão 0.11.0 (28/10/2026). A 1.0 sai quando todos os itens estiverem
**FEITO** ou registrados em "Diferenças inevitáveis" no README como risco aceito.

Legenda: **FEITO**, **PARCIAL** (existe, mas incompleto), **FALTA**.

| Seções | FEITO | PARCIAL | FALTA |
|---|---|---|---|
| 6.1–6.6 (contas, pasta, cache, sincronização, backup, Drives compartilhados) | 70 | 0 | 1 |
| 6.7–6.11 (janela principal, bandeja, pesquisa, Preferências, primeiro uso) | 79 | 0 | 1 |
| 6.12–6.17 (Dolphin, compartilhar e versões, notificações, diagnóstico, políticas, MCP) | 72 | 0 | 2 |

Os quatro itens em FALTA (NAS, atalho global pelo KGlobalAccel, "Adicionar ao espelho" e
"Perguntar ao Gemini" no Dolphin) estão em "Diferenças inevitáveis" no README.

Os pacotes `.deb` e `.rpm` exigidos pela 1.0 já saem em cada release (`packaging/nfpm.yaml`,
`.github/workflows/release.yml`).

## Plano até a 1.0

| Versão | Entrega |
|---|---|
| 0.7 | Dolphin completo: ícones de status, submenu completo (abrir no Docs/Planilhas/Apresentações, copiar link dizendo o tipo de acesso), janelas "Compartilhar" e "Gerenciar versões", exclusão do Baloo ✅ |
| 0.8 | Várias contas (até 4), uma montagem por conta, `DisableAddAccount`; Drives compartilhados na montagem, "Gerenciar drives compartilhados" e criar drive; "Adicionar atalho" no menu, escolhendo o local ✅ |
| 0.9 | Backup de pastas ("Meu computador"), pastas conectadas e "Computadores", modo Espelho com troca entre modos, dispositivos removíveis ✅ (NAS fica como diferença) |
| 0.10 | Diagnóstico, registro detalhado e feedback; notificações restantes e aviso de nova versão; servidor MCP; políticas restantes; ransomware e recuperação do banco ✅ |
| 0.11 | Últimos itens da janela, da bandeja e das Preferências; tour de boas-vindas; limites do Drive com mensagem própria ✅ |
| 1.0 | Matriz toda conferida, "Diferenças inevitáveis" no README |


Legenda: FEITO, PARCIAL, FALTA. Caminhos relativos a `crates/`. Resumo geral: até 4 contas, cada uma com a própria montagem (Meu Drive e Drives compartilhados); modo Stream ou Espelho, pastas do computador em "Computadores" e backup de dispositivos.

### 6.1 Contas e login

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Login pelo navegador (xdg-open), loopback, PKCE | FEITO | `gdl-drive/src/oauth.rs` (`LoginFlow::start`, PKCE S256, 127.0.0.1); `gdl-common/src/browser.rs` (xdg-open) |
| Tempo limite de 5 min | FEITO | `daemon.rs:56` `LOGIN_TIMEOUT = 300 s`; `LoginError::Timeout` |
| Botão "Gerar link" | FEITO | `locales/pt-BR/ui.ftl:235` `onboarding-generate-link`; `oauth.rs:197` `auth_url` |
| Página de resultado própria | FEITO | `oauth.rs:261` `result_page` (login-success/login-failure) |
| Até 4 contas simultâneas | FEITO | `daemon/accounts.rs` (`MAX_ACCOUNTS = 4`), uma montagem por conta (`<pasta> (<e-mail>)` a partir da segunda); testes `second_account_mounts_beside_the_first`, `fifth_account_is_refused` |
| Adicionar, alternar, reautenticar, desconectar uma/todas | FEITO | `home/account.js` (lista de contas, trocar, "Adicionar outra conta", "Entrar de novo"), `more.js` "Desconectar todas as contas" (`LogoutAll`); testes `signing_in_again_resumes_the_queue`, `logout_of_one_keeps_the_other_mounted` |
| Erros com mensagem própria | FEITO | Timeout, Denied, sem refresh token, "Sem conexão", conta já conectada, máximo atingido, outra conta no "entrar de novo", `DisableAddAccount`, organização que bloqueia o app (`admin_policy_enforced`, `org_internal`) e conta desativada (`onboarding-login-failed-*`); "O login está demorando?" depois de 60 s, com "Gerar link de novo" e os problemas comuns (`gdl-drive/src/oauth.rs`, `onboarding.js`) |
| Refresh token no Secret Service | FEITO | `gdl-drive/src/token_store.rs` (chaveiro do sistema) |
| Fluxo de desconexão (contagem, 3 escolhas, confirmação, aviso off-line, progresso) | FEITO | `ui/screens/home/disconnect.js` (finish/move/discard com checkbox de confirmação, `disconnect-offline-warning`, `disconnect-progress`); `daemon.rs:logout` |
| Política `AllowedAccountsPattern` | FEITO | `daemon.rs:781` `allowed()`; teste em `dbus_tests.rs` |
| Política `DisableAddAccount` | FEITO | `begin_login` recusa `add-account-disabled` com uma conta conectada; o menu esconde "Adicionar outra conta" (`can_add_account`); teste `disable_add_account_keeps_only_the_first` |

### 6.2 Pasta do Drive: Stream e Espelho

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Stream: montagem FUSE por conta | FEITO | `gdl-fs/src/fs.rs`, uma montagem por conta (`engine_at` acha a conta pelo caminho) |
| Local padrão e `DefaultMountPoint`, aviso/alternativo se ocupado | FEITO | `daemon/mount.rs`, `daemon.rs:834` (`mount-moved`), teste `busy_mount_point_falls_back_to_numbered_folder`. O padrão por e-mail depende de multi-conta |
| Conteúdo da montagem | FEITO | `gdl-fs/src/layout.rs`: Meu Drive (fora dela no Espelho), Drives compartilhados, Computadores, lixeira, Lost and Found, Uploads cancelados, `.shortcut-targets-by-id` |
| Nomes pelo idioma / `DisableLocalizedVirtualFolders` | FEITO | Nomes via i18n (`Layout::new`); com a política, `daemon.rs` usa os nomes em inglês |
| "Locais" do Dolphin (`ShowMountPointInSidebar`) | FEITO | `gdl-daemon/src/desktop.rs`, `daemon.rs:477` |
| Modo Espelho (pasta real, inotify + changes.list) | FEITO | `gdl-folders` (comparação em três vias, `watch.rs` com inotify e `ItemsChanged`), `daemon/mirror.rs`; documentos Google como ponteiros e atalhos como links |
| Drives compartilhados sempre em Stream | FEITO | Os drives ficam na montagem (`Node::SharedDrives`); não há modo Espelho para eles |
| Troca Espelho <-> Stream com salvaguardas | FEITO | `MirrorCheck` (tamanho, espaço livre, pendências), `SetStream` recusa com envios pendentes e manda a pasta antiga para a lixeira só se pedido; aba Google Drive |
| Sem limite de 4 GB | FEITO | Sem limite no FUSE; upload retomável (`uploads.rs`) |
| Documentos Google como ponteiros, abrir no navegador, copiar cria atalho + aviso | FEITO | `gdl-common/src/gdocs.rs`, `gdl-engine/src/gdocs.rs`, `gdl-cli open-pointer`, `notify-shortcut-*` |
| Atalhos do Drive como symlinks para `.shortcut-targets-by-id` | FEITO | `fs.rs:224,386` (`FileType::Symlink`, `link_target`) |
| `/` como `∕` e duplicados com sufixo estável | FEITO | `gdl-common/src/names.rs` (`to_local`, `to_remote`, `dedupe`) |
| Excluir manda para a lixeira do Drive | FEITO | `gdl-fs/src/trash.rs`, `fs.rs` (lookup_trashed); teste em `write_tests.rs` |

### 6.3 Arquivos sob demanda, cache e off-line

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Metadados de todo o Drive em SQLite por conta | FEITO | `gdl-store/src/lib.rs`, `schema.rs`, `gdl-engine/src/indexer.rs` |
| Leitura em blocos, tabela de intervalos, readahead sequencial | FEITO | `gdl-engine/src/cache.rs` (blocos 1 MiB, janelas 8 a 64 MiB; testes `sequential_read_uses_growing_windows`, `random_seek_downloads_only_nearby_blocks`) |
| "Disponível off-line" em arquivos/pastas, mantém atualizado, "Somente on-line", estado misto | FEITO | `gdl-engine/src/pins.rs`; `offline.js` (`offline-state-mixed`) |
| Gravações no cache + fila persistente, uploads retomáveis | FEITO | `uploads.rs` (`recover_on_start`), `gdl-store`, `no_data_loss_tests.rs` |
| Cache: `ContentCachePath` com migração e progresso | FEITO | `engine.rs:95`, `daemon.rs:751` (notificação `cache-moved` moving/moved) |
| Cache: `ContentCacheMaxKbytes` sem contar fixados/pendentes | FEITO | `cache.rs` teste `eviction_respects_limit_and_skips_pinned_and_dirty` |
| Cache: `MinFreeDiskSpaceKBytes` (500 MB), para de gravar e avisa | FEITO | `config.rs:122` (512000 KB), `cache.rs:473 check_disk`, `notify-disk-low` |
| Cache: remoção por uso menos recente | FEITO | `cache.rs:547 evict_if_needed`, `store.touch_access` |
| Tela de arquivos off-line / armazenamento | FEITO | `ui/screens/offline.js` (barra segmentada, breadcrumb, remover, Limpar cache, Limpar off-line, atalho às configurações). Uso "por conta" = conta única |
| Sem internet: acessa cache/fixados, EIO claro, notificação "Sem conexão", edições na fila | FEITO | `cache.rs` teste `offline_with_missing_block_is_offline_error_but_cached_block_still_reads`; `notify-offline-*` |
| Pausar e retomar (bandeja e janela) | FEITO | `engine.rs set_paused`, `tray-pause`/`action-pause` |
| Pausar em conexão limitada (NetworkManager) | FEITO | `gdl-daemon/src/metered.rs`, `daemon.rs:416 watch_metered` |
| Salvar com sync pausada pergunta se quer retomar | FEITO | `notify-paused-*` com ação "Retomar" (`main.ftl:34-36`) |

### 6.4 Sincronização, conflitos e recuperação

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Envio/recebimento automáticos, progresso por item | FEITO | `gdl-engine/src/activity.rs` (`ActivityState`: fila, pausado, enviando %, enviado, cancelando, cancelado, baixando, lixeira, restaurando); com a sincronização pausada, a fila aparece como "pausado" |
| Cancelar upload, aviso "concluiu antes", pasta "uploads cancelados" | FEITO | `uploads.rs CancelOutcome` (Canceled/TooLate/InUse), `kept.rs` |
| Detecção de conflito por revisão-base | FEITO | `schema.rs`, `changes.rs` (teste `remote_edit_over_dirty_item_marks_conflict`) |
| Stream: nuvem no caminho original, edição em Lost and Found, notificação com "Ver arquivos" e "Mover tudo para…" | FEITO | `uploads.rs`, `notify.rs` (`OPEN_LOST_FOUND` e `MOVE_LOST_FOUND`); "Mover tudo para…" pede a pasta na janela e chama `MoveLostAndFound` (`gdl-daemon/src/daemon/lost_found.rs`), sem sobrescrever nada |
| Envio impossível (sem permissão etc.) vai para Lost and Found | FEITO | teste `permission_denied_goes_to_lost_and_found` |
| Excluído na nuvem com edição pendente: não recria, Lost and Found | FEITO | teste `remote_delete_during_pending_edit_goes_to_lost_and_found_and_is_not_recreated` |
| Espelho: duas versões lado a lado com sufixo de conflito | FEITO | `gdl-folders/src/decide.rs` (` (conflito AAAA-MM-DD HHhMM)`), teste `both_edited_keeps_both_versions_everywhere` |
| Recuperar arquivos não sincronizados | FEITO | `uploads.rs:234 recover_on_start`; Lost and Found e pendentes copiáveis no logout `--move` |
| Recuperação automática após falha do banco local | FEITO | `gdl-store/src/salvage.rs` (`quick_check` na abertura), `gdl-engine/src/recovery.rs`; teste `corrupt_database_keeps_pending_content` |
| Detecção de ransomware (entropia, pausa, aviso com 3 opções) | FEITO | `gdl-engine/src/ransomware.rs` (entropia > 7,6, 20 em 60 s, formatos comprimidos fora), pausa própria e aviso com as 3 opções; testes `twenty_encrypted_files_pause`, `compressed_formats_do_not_count` |
| Limites do Drive em mensagens claras | FEITO | Cota cheia, limite diário de upload, limite de download, pasta além de 100 níveis, pasta ou drive compartilhado no limite de 500 mil itens e sem permissão têm categoria e texto (`gdl-engine/src/errors.rs`, `gdl-drive/src/client.rs`) |
| Lista de erros: categorias, paginação, ações por item | FEITO | `ui/screens/errors.js` + `errors/model.js` (10 por página, Abrir pasta, Tentar de novo) |
| Lista de erros: aviso de erros em outra conta + trocar de conta | FEITO | `AccountSummary.errors`; a lista de erros mostra "Há N erros em <e-mail>" para cada outra conta, com "Trocar de conta" (`errors.js`) |

### 6.5 Backup de pastas do computador e dispositivos

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Aba "Meu computador": adicionar pasta | FEITO | `ui/screens/preferences.js` `computerTab`, `AddLocalFolder` |
| Configurador por pasta (conta, destino, tamanho, contagem) | FEITO | `addFolder` + `CheckLocalFolder` (recusas com texto próprio) |
| Parar a sincronização com confirmação | FEITO | `folder-actions.js` `stopFolder`; os arquivos ficam nos dois lados |
| Mesclar com pasta existente | FEITO | `AddLocalFolder(merge)` com a pasta de mesmo nome em "Computadores/<este computador>" |
| Pastas XDG sugeridas | FEITO | Passo de pastas do primeiro uso, com o tamanho de cada uma |
| Pastas conectadas bidirecionais (destino "Computadores") | FEITO | `daemon/folders.rs`, pastas de apoio achadas por `appProperties`; "Computadores" na montagem |
| Avisos: pasta sumiu/movida/sem permissão | FEITO | `notify::for_folder` (e exclusão em massa pedindo confirmação), "Escolher novo local" |
| Proibir aninhar pastas sincronizadas | FEITO | `gdl-folders/src/guard.rs` (`overlap`, recusas `nested`/`contains`) |
| Dispositivos removíveis via UDisks2 (notificação, lembrar, ignorados, backup único, "seguro remover") | FEITO | `gdl-daemon/src/devices.rs` e `daemon/devices.rs` (`Devices`, `BackupDevice`, `IgnoreDevice`, `ForgetDevice`); o aviso tem "Fazer backup", "Fazer backup sempre" e "Ignorar este dispositivo" (`DEVICE_ALWAYS`, `DEVICE_IGNORE`) |
| `DisableExternalMediaSync` e preferência "perguntar sobre dispositivos" | FEITO | `devices_refused`, `PromptToBackupDevices` na aba "Meu computador" |
| Compartilhamentos de rede (NAS), backup a cada 6 h | FALTA | Volumes de rede e FUSE são recusados (`guard::network_fs`); registrado em "Diferenças inevitáveis" |
| `DisableMirroredMyDrive` e `DisableMirroredFolders` | FEITO | `mirror_refused`, `folders_refused` (pares guardados não rodam com a política) |

### 6.6 Drives compartilhados, "Compartilhados comigo" e atalhos

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Drives compartilhados aparecem na montagem | FEITO | `layout.rs` (`Drives compartilhados/<nome>`), `client.rs` (`supportsAllDrives`, `drives`); nomes repetidos ganham o fim do id (`drive_dir_names`); testes `shared_drive_folder_lists_enabled_drives`, `same_named_drives_get_distinct_folders` |
| Tela "Gerenciar drives compartilhados" (ligar/desligar, salvar/cancelar) | FEITO | `ui/screens/shared-drives.js` (Preferências e menu na pasta "Drives compartilhados"); desligar com envio pendente é recusado; e2e `shared_drives_screen_saves_and_creates` |
| Aviso de drive com alta atividade | FEITO | Mais de 1 000 mudanças numa consulta avisam `busy-shared-drive`, no máximo uma vez por dia por drive (`gdl-engine/src/changes.rs`); família que pode ser desligada |
| Criar Drive compartilhado (tela e menu de contexto, nome único) | FEITO | `CreateSharedDrive` (nome único com " (2)"), "Novo Drive compartilhado" na tela e no menu da pasta "Drives compartilhados"; teste `new_drive_names_are_unique` |
| "Compartilhados comigo": lista com data | FEITO | `ipc SharedItem.shared_ms`, `ui/screens/home/shared.js`, `search.rs:74` |
| "Adicionar atalho" no local escolhido | FEITO | Tela `shortcut.js` (Meu Drive e drives que aceitam itens, descendo pelas pastas), aberta pelo menu e por "Compartilhados comigo"; teste `shortcut_goes_to_the_chosen_folder`, e2e `shortcut_screen_picks_a_folder` |
| "Abrir na web" | FEITO | `shared.js:23` (`open_item` target `web`) |
| Avisos ao mover entre Drives compartilhados/pastas compartilhadas (com "não mostrar de novo") | FEITO | `moved-to-shared-drive` e `moved-to-shared-folder` (pasta do Meu Drive que é de outra pessoa, pelo `ownedByMe`), as duas famílias desligáveis (`gdl-engine/src/ops.rs`) |
| Mover pasta do Meu Drive para Drive compartilhado: não suportado e orienta web | FEITO | `Engine::move_item` (`FolderCrossDrive` → `EPERM`) e aviso `folder-move-unsupported`; teste `moving_a_folder_into_a_shared_drive_is_refused` |


Frontend: `crates/gdl-ui/ui/` (JS puro). Bandeja: `crates/gdl-tray/src/{model,main,hotkey}.rs`. Textos: `locales/*/{main,ui}.ftl`.

### 6.7 Janela principal

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Cabeçalho: status (concluída, incompleta/erro, sincronizando N + tempo restante, sem internet, armazenamento cheio) | FEITO | `home/model.js` `headerFor` (TITLES, `header-remaining`, `-error-detail`, `-offline-detail`, `-quota-detail`) |
| "Última sincronização há…" | FEITO | `home/model.js` `lastSync` |
| Botão Pausar/Retomar | FEITO | `home.js` `togglePause` |
| Campo "Pesquisar no Google Drive" | FEITO | `home.js` `layout` (input `.search`, Enter abre tela `search`) |
| Menu da conta: foto + uso de armazenamento | FEITO | `home/account.js` `accountMenu` (avatar, nome, e-mail, uso/total) |
| Adicionar conta / trocar de conta | FEITO | `home/account.js` (lista com foto, e-mail e estado; trocar; adicionar); e2e `account_menu_switches_and_adds_accounts` |
| Desconectar | FEITO | `home/disconnect.js` (com escolhas p/ pendências) |
| Gerenciar armazenamento, Privacidade, Termos | FEITO | `account.js` (`openLink('storage'/'privacy'/'terms')`) |
| Início: cartão Precisa de atenção | FEITO | `home/start.js` `attention` |
| Início: Arquivos off-line, Compartilhados comigo, Status de sincronização, Gemini | FEITO | `home/start.js` `render` |
| Início: Drives compartilhados | FEITO | Cartão de Drives compartilhados no Início (`home/model.js` `drivesCard`) |
| Início: Pastas conectadas | FEITO | Cartão "Pastas conectadas" (`foldersCard`) |
| Atividade: tabela paginada e ordenável | FEITO | `home/activity.js` (`sortRows`, `paginate`, tamanhos 10/25/50) |
| Atividade: menu por linha (abrir no gerenciador, abrir na web, copiar link, cancelar upload) | FEITO | `activity.js` `rowMenu` |
| Atividade: "Compartilhar" (só após upload) | FEITO | `rowActions` no menu da linha: "Compartilhar" só em item já no Drive |
| Visão Pastas conectadas | FEITO | Visão com as linhas de `folderRows` e o mesmo menu das Preferências |
| Visão Arquivos off-line | FEITO | `home/offline.js` + tela `offline.js` |
| Visão Compartilhados comigo | FEITO | `home/shared.js` (Adicionar atalho / Abrir na web) |
| Visão Drives compartilhados | FEITO | Visão `drives` da janela principal (`home/drives.js`): abrir a pasta ou na web e "Gerenciar drives compartilhados" |
| Notificações com contador de não lidas | FEITO | `home/notifications.js`, `home.js` `setUnread` / `updateBadge` |
| Links rápidos (Drive web, FAQ, ajuda off-line) | FEITO | `home/links.js` |
| Mais opções: Arquivos off-line, Lista de erros (contagem), Preferências, Sobre | FEITO | `home/more.js` `moreMenu` |
| Mais opções: Gerenciar drives compartilhados | FEITO | Item do menu Mais opções (`more.js`) |
| Mais opções: Enviar feedback | FEITO | `more.js` → tela `feedback` |
| Mais opções: registro detalhado liga/desliga | FEITO | `more.js` `verbose` (`set_verbose_logging`) |
| Mais opções: gerar informações de diagnóstico | FEITO | `more.js` `diagnostics` (`generate_diagnostics`) |
| Mais opções: Redefinir preferências | FEITO | `more.js` `reset` -> `reset_preferences` |
| Mais opções: Desconectar todas as contas | FEITO | `more.js` `disconnect-all` → `logout_all`, com o fluxo de desconexão |
| Mais opções: Sair com confirmação se sincronizando | FEITO | `more.js` `quit` |
| Extra (fora da spec): preferências de notificação | FEITO | `more.js` + `notification-settings.js` |
| Atalho de teclado que foca o painel de status | FEITO | `home.js` `wire`: Ctrl+Shift+Alt+G -> `ui.header.focus()` |

### 6.8 Ícone na bandeja

| Item | Situação | Evidência / o que falta |
|---|---|---|
| StatusNotifierItem via ksni | FEITO | `gdl-tray/src/main.rs` (impl `ksni::Tray`) |
| Ícones: sincronizado, sincronizando (animado), erro, pausado, sem internet, sem login | FEITO | `tray/model.rs` `SYNC_ICONS`/`STILL_ICONS`; SVGs `assets/icons/hicolor/scalable/status/gdl-*.svg`; `animated()`/`icon_at` |
| Tooltips: Tudo sincronizado, Sincronizando N, Sem internet (N), Pausado (N), Armazenamento cheio, Não fez login | FEITO | `model.rs` `from_status`; `main.ftl` tray-* |
| Tooltip "Conectado com N contas" | FEITO | `tray/model.rs` (`status.accounts.len() > 1`) |
| Clique esquerdo abre a janela principal | FEITO | `model.rs` `click: Action::Open("home")` (sem login: onboarding) |
| Menu direito: Abrir pasta, Abrir no navegador, Pesquisar, Pausar/Retomar | FEITO | `model.rs` `items` |
| Menu direito: Arquivos off-line, Lista de erros (contagem), Preferências, Sobre, Sair | FEITO | `model.rs` `items` |
| Menu direito: Enviar feedback, diagnóstico | FEITO | `model.rs` `tray-feedback`, `Action::GenerateDiagnostics` |
| Menu direito: Gerenciar drives compartilhados, registro detalhado, Redefinir preferências, Desconectar todas | FEITO | `gdl-tray/src/model.rs`: os quatro itens; os que pedem confirmação abrem a janela (`home:reset-preferences`, `home:disconnect-all`) |

### 6.9 Pesquisa rápida

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Janela flutuante | FEITO | `gdl-ui/src/screens.rs` (`search`, `floating: true`) |
| Atalho global Ctrl+Alt+G configurável via portal XDG GlobalShortcuts | FEITO | `gdl-tray/src/hotkey.rs` (`ashpd` GlobalShortcuts, `parse_trigger`), `main.rs` `keep_search_hotkey`; padrão `Ctrl+Alt+G`; configurável em Preferências (`SearchHotKey`) |
| Fallback KGlobalAccel | FALTA | Nenhuma ocorrência de KGlobalAccel no código |
| Pesquisa por nome (banco local) e conteúdo (Drive API `fullText`), sem rede só nomes | FEITO | `gdl-engine/src/search.rs` `search` (`search_names` + `client.search`, timeout, cai p/ banco) |
| Resultados com ícone, nome e caminho | FEITO | `ui/screens/search.js` `draw`, `search/model.js` `iconFor`/`folderOf` |
| Sugestões recentes com campo vazio | FEITO | `search.js` `ask` -> `recent_items`; `search-recent` |
| Mensagem "Nenhum item corresponde a…" | FEITO | `search.js` `search-none` |
| Abrir resultado pela montagem (arquivo ou pasta) | FEITO | `search.js` `open` -> `open_item target:'file'` |
| "Na conta selecionada" | FEITO | `api.call` leva a conta mostrada (`state.js` `currentAccount`) |

### 6.10 Preferências

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Duas abas "Meu computador" e "Google Drive" | FEITO | `preferences.js`: duas abas e a engrenagem, que abre as configurações avançadas com "Voltar" |
| Aba "Meu computador" (seção 6.5: pastas do computador) | FEITO | `computerTab`: pastas e dispositivos |
| Aba Drive: modo Stream | FEITO | `driveTab` |
| Aba Drive: modo Espelho | FEITO | `driveTab` `toMirror`/`toStream` (a troca vale ao confirmar o diálogo) |
| Aba Drive: local da pasta do Meu Drive e abrir pasta | FEITO | `driveTab` (`form.mount`, `api.openMount`); mudar local em `DefaultMountPoint` (`settingsTab`) |
| Rodapé: Alterações pendentes, Cancelar, Salvar | FEITO | `draw`/`updateFooter`/`save`/`cancel` |
| Aviso "exige reiniciar" + "Reiniciar agora" | FEITO | `ui.banner`, `api.call('restart')` |
| Local do streaming + mostrar em Locais do Dolphin | FEITO | `DefaultMountPoint`, `ShowMountPointInSidebar` |
| Pasta de cache (Alterar/Redefinir) + limite (1 até máximo) | FEITO | `folder('ContentCachePath')`, `cacheLimit` (máx. = cache + livre) |
| Ver armazenamento do cache | FEITO | botão `pref-cache-storage` -> tela `offline` |
| Iniciar ao fazer login | FEITO | `AutoStartOnLogin` |
| Proxy automático / direto / manual | FEITO | `proxy()`, `proxyMode`/`proxyPatch` |
| Limites de banda download/upload KB/s | FEITO | `limit('BandwidthRxKBPS'/'BandwidthTxKBPS')` |
| Atalho da pesquisa | FEITO | `hotkey()` (`SearchHotKey`) |
| Relatório automático de erros (desligado por padrão) | FEITO | `AutomaticErrorReporting` checkbox |
| Perguntar sobre backup de dispositivos + lista de ignorados | FEITO | Aba "Meu computador": `PromptToBackupDevices` e a lista com "Ignorar" e "Remover" |
| Pausar em conexão limitada | FEITO | `PauseSyncingOnMeteredConnection`; `daemon/src/metered.rs` |
| Tema (sistema ou claro) | FEITO | `theme()` (`UITheme`) |
| Desconectar a conta | FEITO | `settingsTab` seção `pref-account` |
| Itens por política bloqueados "Gerenciado pelo administrador" | FEITO | `row()` (fieldset disabled + `pref-managed`), `fromPrefs` `managed` |
| Tour de boas-vindas opcional | FEITO | `ui/tour/`: tour da janela principal, oferecido no fim do primeiro uso e em Preferências > Outros |

### 6.11 Primeiro uso

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Passos em lista vertical | FEITO | `onboarding.js` `draw` (`ol.steps`) |
| 1. Entrar / criar conta | FEITO | passos `setup` (guia de client OAuth) e `sign-in` |
| 2. Aguardando login com "Gerar link" | FEITO | passo `waiting`, botão `onboarding-generate-link` |
| 3. Apresentação | FEITO | passo `intro` |
| 4. "Veja seus arquivos no Dolphin" com botão abrir | FEITO | passo `files` (`api.openMount`) |
| 5. Escolher pastas para sincronizar (sugestões e tamanho) | FEITO | Passo `folders` do primeiro uso |
| 6. Resumo | FEITO | Passo Resumo do primeiro uso (`summaryRows`): conta, modo, pasta e pastas escolhidas |
| 7. Como tornar disponível off-line | FEITO | passo `offline` |
| Botões Pular, Voltar, Continuar | FEITO | `footer`, `actions` em `onboarding/model.js` |
| Política `DisableOnboardingDialog` | FEITO | `gdl-common/src/config.rs:119`; `onboarding.js` `loginOnly`; `stepsFor` |


Legenda: FEITO / PARCIAL / FALTA. Caminhos relativos à raiz do repositório.

### 6.12 Integração com o gerenciador de arquivos

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Ícones de status (KOverlayIconPlugin) | FEITO | `integrations/dolphin/gdriveoverlay.cpp` (emblemas `gdl-emblem-{cloud,offline,syncing,conflict,error}` pelo xattr `user.gdl.state`, atualizados pelo sinal D-Bus `StatesChanged`). Outros gerenciadores seguem sem ícones. |
| Abrir com Drive web | FEITO | `Action::OpenWeb`, `crates/gdl-daemon/src/menu.rs:43`. |
| Abrir com Docs / Planilhas / Apresentações | FEITO | `Action::OpenDocs/OpenSheets/OpenSlides` para arquivos do Office (`menu.rs`, `gdl-engine/src/gdocs.rs` `OfficeEditor`). |
| Compartilhar | FEITO | `Action::Share` abre a janela `share:<id>` (`ui/screens/share.js`). |
| Copiar link (com aviso público/domínio/restrito) | FEITO | `Action::CopyLink` + `notify_link_copied` (`daemon/sharing.rs`, textos `notify-link-*`). |
| Acesso off-line / Somente on-line (marca mista) | FEITO | `menu.rs:44-46,103-111` (`Offline::Mixed`), plugin Dolphin `gdrivemenu.cpp:93`. |
| Adicionar atalho ao Google Drive | FEITO | `menu.rs` `Action::AddShortcut` (itens que não são atalhos) abre a tela `shortcut` |
| Gerenciar versões | FEITO | `Action::Versions` abre a janela `versions:<id>` (só arquivos com versões). |
| Ver lixeira | FEITO | `Action::Trash` (`menu.rs:51,153`). |
| Atualizar pasta | FEITO | `Action::Refresh`. |
| Enviar ou sincronizar esta pasta (fora da montagem) | FEITO | `Action::SyncFolder` abre o configurador; emblemas nas pastas sincronizadas (`FolderStates`). |
| Adicionar ao espelho da conta 1–4 | FALTA | Serve ao Espelho de pastas escolhidas, que não existe no Linux; registrado em "Diferenças inevitáveis". |
| Gerenciar / Criar drive compartilhado | FEITO | `menu.rs` `NewSharedDrive`/`ManageSharedDrives` na pasta "Drives compartilhados" |
| Criar novo documento, planilha, apresentação | FEITO | `Action::NewDocument/NewSpreadsheet/NewPresentation`, `menu.rs:47-49,67-77`. |
| Perguntar ao Gemini (web) | FALTA | Nenhuma ação no menu (só cartão Gemini na janela principal). |
| Miniaturas sem baixar (thumbnailLink) | FEITO | `crates/gdl-engine/src/thumbs/{seeder,freedesktop}.rs` (semeia o cache freedesktop com o `thumbnailLink`; não é um thumbnailer KIO próprio). |
| Excluir montagens do Baloo no primeiro uso | FEITO | `desktop.rs` `set_baloo_exclusion` (a cada montagem, numa sessão KDE; desfeito ao desinstalar). |
| Outros gerenciadores: ações básicas, sem ícones | FEITO | `integrations/gtk/gdrive_desktop_linux_menu.py` (Nautilus/Nemo/Caja/Thunar), `crates/gdl-daemon/src/cosmic.rs`. |

### 6.13 Compartilhamento, versões, lixeira e criação

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Janela "Compartilhar": adicionar pessoas com papel e mensagem | FEITO | `ui/screens/share.js`, D-Bus `share_add`/`share_set_role`, `gdl-drive/src/client/sharing.rs`. |
| Janela "Compartilhar": acesso geral (restrito/domínio/link) | FEITO | `share_set_general` → `Engine::set_general_access` (troca tira os de outro tipo). |
| Janela "Compartilhar": copiar link | FEITO | Pelo menu (`CopyLink`) e pelo botão da janela. |
| Abrir a janela pela MCP (corrigir permissão de um e-mail) | FEITO | `open_acl_dialog` → D-Bus `OpenShare` abre a janela Compartilhar com o e-mail digitado; teste `acl_dialog_only_opens`. |
| Gerenciar versões (baixar, restaurar, manter para sempre; mensagens) | FEITO | `ui/screens/versions.js`, D-Bus `version_download/restore/keep`, `gdl-engine/src/sharing.rs` (restaurar vira edição local que sobe como versão nova). |
| Lixeira: pasta somente leitura + Restaurar | FEITO (variante) | Lixeira KIO `.Trash-<uid>` na montagem (`gdl-fs/src/trash.rs`); restaurar = `engine.untrash` (`gdl-fs/src/write.rs:417-421`); 30 dias fica por conta do Drive. Não é uma pasta virtual própria com ação "Restaurar" do app. |
| Criar Docs/Planilhas/Apresentações na pasta e abrir no navegador | FEITO | `menu.rs:67-77` + `MenuAction`; `menu_action` em `gdl-cli/src/menu_action.rs`. |

### 6.14 Notificações

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Notificações do sistema com ações + lista na janela | FEITO | `gdl-daemon/src/notify.rs` (`Notifier`, `org.freedesktop.Notifications`), D-Bus `Notifications/InvokeNotificationAction` (`dbus.rs:62-74`), tela em `ui/screens/home/notifications.js`. |
| Erros fatais e de conta | FEITO | `Offline`, `UploadFailed`, login expirado (`NeedsSignIn`, com "Entrar") e conta bloqueada pela organização (`AccountDisabled`, 403 `domainPolicy`). |
| Armazenamento e cota | FEITO | `QuotaFull`, `DiskLow`, `QuotaAlmostFull` (90 % e 95 %, `gdl-engine/src/health.rs`) e `LocalDiskFull` (`ENOSPC` ao gravar). |
| Sincronização e pastas | FEITO | `LostAndFound`, `SavedWhilePaused`, `UploadCanceled`, `CopiedAsShortcut`, pastas conectadas, exclusão em massa, `ItemsRecovered`, `UploadsStuck` (3 dias) e itens movidos para a lixeira (`notify::items_trashed`). |
| Permissões e movimentações | FEITO | `NoPermission` (criar, mover, renomear, excluir; a montagem responde `EACCES`), Drive compartilhado com muita atividade e mover para pasta de outra pessoa |
| Dispositivos (backup?, seguro remover, backup concluído/erro) | FEITO | `notify::device_ask`/`device_done` (um aviso por dispositivo). |
| "Nova versão do app" (releases do GitHub) | FEITO | `gdl-daemon/src/updates.rs`, `daemon/releases.rs` (1 min depois da partida e a cada 24 h; cada versão avisa uma vez). |
| "Não mostrar de novo" + tela Preferências de notificação | FEITO | `dbus.rs:78-88`, `ui/screens/notification-settings.js`. |

### 6.15 Diagnóstico, feedback e Sobre

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Registro detalhado liga/desliga | FEITO | `gdl-daemon/src/logging.rs` (arquivo girando 5 × 10 MB; detalhado por 24 h), D-Bus `SetVerboseLogging`, Mais opções. |
| Gerar informações de diagnóstico (pacote de logs) | FEITO | `gdl-daemon/src/diagnostics.rs` (`scrub`, `.tar.gz` 0600), D-Bus `GenerateDiagnostics`, Mais opções e bandeja. |
| Enviar feedback (issue pré-preenchida no GitHub + anexo) | FEITO | `ui/screens/feedback.js` (Problema ou Sugestão, diagnóstico opcional, issue pré-preenchida). |
| Relatório de falhas local, enviado com consentimento | FEITO | `gdl-common/src/crash.rs` (20 mais novos, 0700/0600); com `AutomaticErrorReporting`, aviso "Enviar relatório" que abre a issue filtrada. |
| Sobre (versão, GPL-3.0, terceiros, não oficial, links) | FEITO | `ui/screens/about.js`, `gdl-ui/src/third_party.rs`, `locales/pt-BR/ui.ftl:9-12`. |
| Redefinir preferências com confirmação | FEITO | `dbus.rs:196` (`reset_preferences`), `ui.ftl:142-150`. |
| Redefinir dados locais com confirmação | FEITO | D-Bus `ResetLocalData` (recusa com envios pendentes), Mais opções. |

### 6.16 Políticas e configuração

| Item | Situação | Evidência / o que falta |
|---|---|---|
| TOML, três camadas e precedência (policies > preferences > defaults) | FEITO | `crates/gdl-common/src/config.rs` (macro `preferences!`, `Layer::over`, `ConfigError::Managed`). |
| AllowedAccountsPattern | FEITO | `daemon.rs:778-784`. |
| AutoStartOnLogin | FEITO | `daemon.rs:458-460`, `desktop.rs`. |
| BandwidthRxKBPS | FEITO | `daemon.rs:1577`, `gdl-drive/src/throttle.rs`. |
| BandwidthTxKBPS | FEITO | `daemon.rs:1578`. |
| ContentCachePath | FEITO | `gdl-engine/src/engine.rs:92-108`. |
| ContentCacheMaxKbytes | FEITO | `engine.rs:168`. |
| DefaultMountPoint | FEITO | `daemon.rs:1032-1035`, `mount.rs`. |
| DefaultWebBrowser | FEITO | `gdl-common/src/browser.rs`, `config.rs:204`. |
| DisableAddAccount | FEITO | `begin_login` e `can_add_account` |
| DisableExternalMediaSync | FEITO | `daemon/devices.rs` (`devices_refused`). |
| DisableLocalizedVirtualFolders | FEITO | `Layout` com os nomes em inglês (`gdl-fs/src/layout.rs`). |
| DisableMirroredMyDrive | FEITO | `daemon/mirror.rs` (`mirror_refused`). |
| DisableMirroredFolders | FEITO | `daemon/folders.rs` (`folders_refused`). |
| DisableOnboardingDialog | FEITO | `gdl-tray/src/main.rs:148`. |
| MinFreeDiskSpaceKBytes | FEITO | `engine.rs:169`. |
| OpenOfficeFilesInDocs | FEITO | Esconde os itens "Abrir no Docs/Planilhas/Apresentações" do menu quando desligada. |
| DirectConnection | FEITO | `daemon.rs:262-264`. |
| TrustedRootCertsFile | FEITO | `gdl-drive/src/lib.rs` (`set_http`, raízes do PEM somadas às do sistema). |
| TrustSystemRootCerts | FEITO | Desligada, valem só as raízes do `TrustedRootCertsFile`. |
| DisableCRLCheck | FEITO | Aceita e sem efeito: o TLS do app não consulta listas de revogação (registrado no CHANGELOG). |
| DisableSSLValidation | FEITO | Só vale vindo do `policies.toml` (`config.rs`, teste `ssl_validation_only_from_policies`); aviso no log a cada partida. |
| SearchHotKey | FEITO | `gdl-tray/src/main.rs:193`, `hotkey.rs`. |
| PauseSyncingOnMeteredConnection | FEITO | `daemon.rs:398,454`, `metered.rs`. |
| AutomaticErrorReporting | FEITO | Ligada, oferece o relatório depois de uma falha (ver 6.15). |
| UITheme | FEITO | `ui/app.js`, `ui/screens/preferences.js`. |
| ShowMountPointInSidebar | FEITO | `daemon.rs:477-483`, `desktop.rs`. |
| PromptToBackupDevices | FEITO | `daemon/devices.rs` (`devices_found`). |

### 6.17 Servidor MCP

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Binário `gdrive-desktop-linux-mcp` (stdio JSON-RPC, D-Bus) | FEITO | `crates/gdl-mcp` (JSON-RPC 2.0, MCP 2025-06-18, stdio), instalado com os outros programas. |
| get_sync_status(path) | FEITO | D-Bus `PathStatus`; teste `status_of_a_synced_path`. |
| add_folder_to_sync(path, sync_mode, account_id?, my_drive_path?) | FEITO | `backup` → `AddLocalFolder`; `stream` → `MoveToDrive` (copia, confere, link no lugar, original na lixeira, desfaz ao falhar); só pastas visíveis da pasta pessoal; testes `stream_move_leaves_a_link`, `stream_move_undoes_on_failure`, `agent_cannot_add_hidden_or_outside_folders`. |
| get_drive_url(path) | FEITO | D-Bus `WebLinkOfPath`; teste `drive_url_of_a_path`. |
| list_tracked_folders() | FEITO | Montagens, Espelho e pastas do computador de cada conta; teste `tracked_folders_lists_pairs_and_mounts`. |
| open_acl_dialog(email, item_id, account_id?) | FEITO | D-Bus `OpenShare` (e-mail já digitado na janela Compartilhar); teste `acl_dialog_only_opens`. |
| Regra: agente nunca altera permissões em silêncio | FEITO | Nenhuma ferramenta compartilha: `open_acl_dialog` só abre a janela e quem confirma é a pessoa. |

