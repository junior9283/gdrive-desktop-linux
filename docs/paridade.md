# Matriz de paridade

Situação de cada item do catálogo da [especificação](superpowers/specs/2026-10-25-gdrive-desktop-linux-design.md)
(seção 6) no código, conferida na versão 0.6.0 (27/10/2026). A 1.0 sai quando todos os itens estiverem
**FEITO** ou registrados em "Diferenças conhecidas" no README como risco aceito.

Legenda: **FEITO**, **PARCIAL** (existe, mas incompleto), **FALTA**.

| Seções | FEITO | PARCIAL | FALTA |
|---|---|---|---|
| 6.1–6.6 (contas, pasta, cache, sincronização, backup, Drives compartilhados) | 35 | 9 | 27 |
| 6.7–6.11 (janela principal, bandeja, pesquisa, Preferências, primeiro uso) | 57 | 4 | 18 |
| 6.12–6.17 (Dolphin, compartilhar e versões, notificações, diagnóstico, políticas, MCP) | 30 | 8 | 37 |

Os pacotes `.deb` e `.rpm` exigidos pela 1.0 já saem em cada release (`packaging/nfpm.yaml`,
`.github/workflows/release.yml`).

## Plano até a 1.0

| Versão | Entrega |
|---|---|
| 0.7 | Dolphin completo: ícones de status, submenu completo (abrir no Docs/Planilhas/Apresentações, adicionar atalho, copiar link dizendo o tipo de acesso), janelas nativas "Compartilhar" e "Gerenciar versões", exclusão do Baloo |
| 0.8 | Várias contas (até 4), uma montagem por conta, `DisableAddAccount`; Drives compartilhados na montagem, "Gerenciar drives compartilhados" e criar drive |
| 0.9 | Backup de pastas ("Meu computador"), pastas conectadas e "Computadores", modo Espelho com troca entre modos, dispositivos removíveis e NAS |
| 0.10 | Diagnóstico, registro detalhado e feedback; notificações restantes e aviso de nova versão; servidor MCP; políticas restantes; ransomware e recuperação do banco |
| 1.0 | Matriz toda conferida, "Diferenças conhecidas" no README |


Legenda: FEITO, PARCIAL, FALTA. Caminhos relativos a `crates/`. Resumo geral: o app é de **uma conta só** (`daemon.rs`: `account: Mutex<Option<Account>>`), só Meu Drive, sem espelho, sem backup de pastas, sem Drives compartilhados.

### 6.1 Contas e login

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Login pelo navegador (xdg-open), loopback, PKCE | FEITO | `gdl-drive/src/oauth.rs` (`LoginFlow::start`, PKCE S256, 127.0.0.1); `gdl-common/src/browser.rs` (xdg-open) |
| Tempo limite de 5 min | FEITO | `daemon.rs:56` `LOGIN_TIMEOUT = 300 s`; `LoginError::Timeout` |
| Botão "Gerar link" | FEITO | `locales/pt-BR/ui.ftl:235` `onboarding-generate-link`; `oauth.rs:197` `auth_url` |
| Página de resultado própria | FEITO | `oauth.rs:261` `result_page` (login-success/login-failure) |
| Até 4 contas simultâneas | FALTA | Só uma conta (`daemon.rs:251`); sem montagem por conta, sem `~/Google Drive (<e-mail>)` |
| Adicionar, alternar, reautenticar, desconectar uma/todas | PARCIAL | Só login e logout da conta única (`daemon.rs:1334`); falta adicionar/alternar/reautenticar |
| Erros com mensagem própria | PARCIAL | Existem Timeout, Denied, sem refresh token, "Sem conexão" (`oauth.rs:87`). Falta: conta já conectada, máximo atingido, conta bloqueada/desativada pela organização, "login demorando" |
| Refresh token no Secret Service | FEITO | `gdl-drive/src/token_store.rs` (chaveiro do sistema) |
| Fluxo de desconexão (contagem, 3 escolhas, confirmação, aviso off-line, progresso) | FEITO | `ui/screens/home/disconnect.js` (finish/move/discard com checkbox de confirmação, `disconnect-offline-warning`, `disconnect-progress`); `daemon.rs:logout` |
| Política `AllowedAccountsPattern` | FEITO | `daemon.rs:781` `allowed()`; teste em `dbus_tests.rs` |
| Política `DisableAddAccount` | FALTA | Chave declarada em `gdl-common/src/config.rs:113`, mas nenhum código a lê |

### 6.2 Pasta do Drive: Stream e Espelho

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Stream: montagem FUSE por conta | PARCIAL | FUSE funcional (`gdl-fs/src/fs.rs`), mas só 1 conta |
| Local padrão e `DefaultMountPoint`, aviso/alternativo se ocupado | FEITO | `daemon/mount.rs`, `daemon.rs:834` (`mount-moved`), teste `busy_mount_point_falls_back_to_numbered_folder`. O padrão por e-mail depende de multi-conta |
| Conteúdo da montagem | PARCIAL | `gdl-fs/src/layout.rs`: Meu Drive, lixeira, Lost and Found, Uploads cancelados, `.shortcut-targets-by-id`. Faltam `Drives compartilhados` e `Computadores` (README admite) |
| Nomes pelo idioma / `DisableLocalizedVirtualFolders` | PARCIAL | Nomes via i18n (`Layout::new`); a política está em `config.rs:116` mas não é lida (nunca força inglês) |
| "Locais" do Dolphin (`ShowMountPointInSidebar`) | FEITO | `gdl-daemon/src/desktop.rs`, `daemon.rs:477` |
| Modo Espelho (pasta real, inotify + changes.list) | FALTA | Só texto desabilitado em `ui/screens/preferences.js:151` ("Chega numa próxima versão"); sem inotify/watcher |
| Drives compartilhados sempre em Stream | FALTA | Não há suporte a Drives compartilhados |
| Troca Espelho <-> Stream com salvaguardas | FALTA | Depende do Espelho |
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
| Envio/recebimento automáticos, progresso por item | PARCIAL | `gdl-engine/src/activity.rs` (`ActivityState`: fila, enviando %, enviado, cancelando, cancelado, baixando, lixeira, restaurando). Falta o estado "pausado" por item |
| Cancelar upload, aviso "concluiu antes", pasta "uploads cancelados" | FEITO | `uploads.rs CancelOutcome` (Canceled/TooLate/InUse), `kept.rs` |
| Detecção de conflito por revisão-base | FEITO | `schema.rs`, `changes.rs` (teste `remote_edit_over_dirty_item_marks_conflict`) |
| Stream: nuvem no caminho original, edição em Lost and Found, notificação com "Ver arquivos" e "Mover tudo para…" | PARCIAL | `uploads.rs:631`, `notify.rs:92` só oferece "Ver arquivos" (`OPEN_LOST_FOUND`). Falta a ação "Mover tudo para…" |
| Envio impossível (sem permissão etc.) vai para Lost and Found | FEITO | teste `permission_denied_goes_to_lost_and_found` |
| Excluído na nuvem com edição pendente: não recria, Lost and Found | FEITO | teste `remote_delete_during_pending_edit_goes_to_lost_and_found_and_is_not_recreated` |
| Espelho: duas versões lado a lado com sufixo de conflito | FALTA | Não há modo Espelho |
| Recuperar arquivos não sincronizados | FEITO | `uploads.rs:234 recover_on_start`; Lost and Found e pendentes copiáveis no logout `--move` |
| Recuperação automática após falha do banco local | FALTA | Sem detecção de corrupção/reconstrução a partir da nuvem em `gdl-store`/`engine.rs` |
| Detecção de ransomware (entropia, pausa, aviso com 3 opções) | FALTA | Nenhuma ocorrência de entropia/ransomware no código |
| Limites do Drive em mensagens claras | PARCIAL | Cota cheia, limite diário de upload e sem permissão têm categoria e texto (`errors.rs`, `client.rs:120-123`). `DownloadQuotaExceeded` é mapeado mas sem mensagem própria. Faltam 100 níveis de pasta e 500 mil itens em Drive compartilhado |
| Lista de erros: categorias, paginação, ações por item | FEITO | `ui/screens/errors.js` + `errors/model.js` (10 por página, Abrir pasta, Tentar de novo) |
| Lista de erros: aviso de erros em outra conta + trocar de conta | FALTA | Depende de multi-conta |

### 6.5 Backup de pastas do computador e dispositivos

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Aba "Meu computador": adicionar pasta | FALTA | `preferences.js:131` só mostra `pref-computer-empty` |
| Configurador por pasta (conta, destino, tamanho, contagem) | FALTA | Não existe |
| Parar a sincronização com confirmação | FALTA | Não existe |
| Mesclar com pasta existente | FALTA | Não existe |
| Pastas XDG sugeridas | FALTA | Não existe |
| Pastas conectadas bidirecionais (destino "Computadores") | FALTA | Não há sync de pastas locais nem pasta `Computadores` |
| Avisos: pasta sumiu/movida/sem permissão | FALTA | Não existe |
| Proibir aninhar pastas sincronizadas | FALTA | Não existe |
| Dispositivos removíveis via UDisks2 (notificação, lembrar, ignorados, backup único, "seguro remover") | FALTA | Nenhuma referência a UDisks |
| `DisableExternalMediaSync` e preferência "perguntar sobre dispositivos" | FALTA | Chave só declarada em `config.rs:115`, sem efeito |
| Compartilhamentos de rede (NAS), backup a cada 6 h | FALTA | Não existe |
| `DisableMirroredMyDrive` e `DisableMirroredFolders` | FALTA | Chaves só declaradas em `config.rs:117-118`, sem efeito |

### 6.6 Drives compartilhados, "Compartilhados comigo" e atalhos

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Drives compartilhados aparecem na montagem | FALTA | `layout.rs` não tem a pasta; `gdl-drive/src/client.rs` sem `drives.list`/`supportsAllDrives` |
| Tela "Gerenciar drives compartilhados" (ligar/desligar, salvar/cancelar) | FALTA | Não existe tela |
| Aviso de drive com alta atividade | FALTA | Não existe |
| Criar Drive compartilhado (tela e menu de contexto, nome único) | FALTA | Não existe (menu em `gdl-daemon/src/menu.rs` só cobre itens do Meu Drive) |
| "Compartilhados comigo": lista com data | FEITO | `ipc SharedItem.shared_ms`, `ui/screens/home/shared.js`, `search.rs:74` |
| "Adicionar atalho" no local escolhido | PARCIAL | `search.rs:80 add_shortcut` põe sempre no Meu Drive (`shortcut-added = Atalho adicionado ao Meu Drive`); não escolhe o local |
| "Abrir na web" | FEITO | `shared.js:23` (`open_item` target `web`) |
| Avisos ao mover entre Drives compartilhados/pastas compartilhadas (com "não mostrar de novo") | FALTA | Nenhum aviso de mudança de permissão no `fs.rs` rename |
| Mover pasta do Meu Drive para Drive compartilhado: não suportado e orienta web | FALTA | Sem tratamento |


Frontend: `crates/gdl-ui/ui/` (JS puro). Bandeja: `crates/gdl-tray/src/{model,main,hotkey}.rs`. Textos: `locales/*/{main,ui}.ftl`.

### 6.7 Janela principal

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Cabeçalho: status (concluída, incompleta/erro, sincronizando N + tempo restante, sem internet, armazenamento cheio) | FEITO | `home/model.js` `headerFor` (TITLES, `header-remaining`, `-error-detail`, `-offline-detail`, `-quota-detail`) |
| "Última sincronização há…" | FEITO | `home/model.js` `lastSync` |
| Botão Pausar/Retomar | FEITO | `home.js` `togglePause` |
| Campo "Pesquisar no Google Drive" | FEITO | `home.js` `layout` (input `.search`, Enter abre tela `search`) |
| Menu da conta: foto + uso de armazenamento | FEITO (1 conta) | `home/account.js` `accountMenu` (avatar, nome, e-mail, uso/total) |
| Adicionar conta / trocar de conta | FALTA | Sem código multi-conta; `accountMenu` só tem Gerenciar armazenamento, Desconectar, Privacidade, Termos. `tray/main.rs:139` passa contas = 0 ou 1 |
| Desconectar | FEITO | `home/disconnect.js` (com escolhas p/ pendências) |
| Gerenciar armazenamento, Privacidade, Termos | FEITO | `account.js` (`openLink('storage'/'privacy'/'terms')`) |
| Início: cartão Precisa de atenção | FEITO | `home/start.js` `attention` |
| Início: Arquivos off-line, Compartilhados comigo, Status de sincronização, Gemini | FEITO | `home/start.js` `render` |
| Início: Drives compartilhados | FALTA | Nenhum cartão nem código de shared drives |
| Início: Pastas conectadas | FALTA | Sem cartão; a aba "Meu computador" é placeholder |
| Atividade: tabela paginada e ordenável | FEITO | `home/activity.js` (`sortRows`, `paginate`, tamanhos 10/25/50) |
| Atividade: menu por linha (abrir no gerenciador, abrir na web, copiar link, cancelar upload) | FEITO | `activity.js` `rowMenu` |
| Atividade: "Compartilhar" (só após upload) | FALTA | `rowMenu` não tem a ação |
| Visão Pastas conectadas | FALTA | `VIEWS` em `home.js` = start, activity, offline, shared, notifications, links |
| Visão Arquivos off-line | FEITO | `home/offline.js` + tela `offline.js` |
| Visão Compartilhados comigo | FEITO | `home/shared.js` (Adicionar atalho / Abrir na web) |
| Visão Drives compartilhados | FALTA | Não existe |
| Notificações com contador de não lidas | FEITO | `home/notifications.js`, `home.js` `setUnread` / `updateBadge` |
| Links rápidos (Drive web, FAQ, ajuda off-line) | FEITO | `home/links.js` |
| Mais opções: Arquivos off-line, Lista de erros (contagem), Preferências, Sobre | FEITO | `home/more.js` `moreMenu` |
| Mais opções: Gerenciar drives compartilhados | FALTA | Ausente |
| Mais opções: Enviar feedback | FALTA | Ausente |
| Mais opções: registro detalhado liga/desliga | FALTA | Ausente |
| Mais opções: gerar informações de diagnóstico | FALTA | Ausente (grep sem resultado) |
| Mais opções: Redefinir preferências | FEITO | `more.js` `reset` -> `reset_preferences` |
| Mais opções: Desconectar todas as contas | PARCIAL | Só "Desconectar" (conta única, `more.js`) |
| Mais opções: Sair com confirmação se sincronizando | FEITO | `more.js` `quit` |
| Extra (fora da spec): preferências de notificação | FEITO | `more.js` + `notification-settings.js` |
| Atalho de teclado que foca o painel de status | FEITO | `home.js` `wire`: Ctrl+Shift+Alt+G -> `ui.header.focus()` |

### 6.8 Ícone na bandeja

| Item | Situação | Evidência / o que falta |
|---|---|---|
| StatusNotifierItem via ksni | FEITO | `gdl-tray/src/main.rs` (impl `ksni::Tray`) |
| Ícones: sincronizado, sincronizando (animado), erro, pausado, sem internet, sem login | FEITO | `tray/model.rs` `SYNC_ICONS`/`STILL_ICONS`; SVGs `assets/icons/hicolor/scalable/status/gdl-*.svg`; `animated()`/`icon_at` |
| Tooltips: Tudo sincronizado, Sincronizando N, Sem internet (N), Pausado (N), Armazenamento cheio, Não fez login | FEITO | `model.rs` `from_status`; `main.ftl` tray-* |
| Tooltip "Conectado com N contas" | PARCIAL | Chave `tray-accounts` e ramo `accounts > 1` existem, mas `main.rs:139` passa só 0/1 (sem multi-conta) |
| Clique esquerdo abre a janela principal | FEITO | `model.rs` `click: Action::Open("home")` (sem login: onboarding) |
| Menu direito: Abrir pasta, Abrir no navegador, Pesquisar, Pausar/Retomar | FEITO | `model.rs` `items` |
| Menu direito: Arquivos off-line, Lista de erros (contagem), Preferências, Sobre, Sair | FEITO | `model.rs` `items` |
| Menu direito: Gerenciar drives compartilhados, Enviar feedback, registro detalhado, diagnóstico, Redefinir preferências, Desconectar todas | FALTA | Não há `Action` nem item para eles (`Action` = Open/OpenFolder/OpenWeb/Pause/Resume/StartService/Quit) |

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
| "Na conta selecionada" | PARCIAL | Só há uma conta |

### 6.10 Preferências

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Duas abas "Meu computador" e "Google Drive" | PARCIAL | `preferences.js` `TABS` tem 3 abas (computer, drive, settings); as avançadas ficam numa aba "Configurações" |
| Aba "Meu computador" (seção 6.5: pastas do computador) | FALTA | `computerTab` é placeholder ("Chega em breve", botão desabilitado) |
| Aba Drive: modo Stream | FEITO | `driveTab` |
| Aba Drive: modo Espelho | FALTA | Radio desabilitado, "em breve" |
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
| Perguntar sobre backup de dispositivos + lista de ignorados | FALTA | Sem controle na tela |
| Pausar em conexão limitada | FEITO | `PauseSyncingOnMeteredConnection`; `daemon/src/metered.rs` |
| Tema (sistema ou claro) | FEITO | `theme()` (`UITheme`) |
| Desconectar a conta | FEITO | `settingsTab` seção `pref-account` |
| Itens por política bloqueados "Gerenciado pelo administrador" | FEITO | `row()` (fieldset disabled + `pref-managed`), `fromPrefs` `managed` |
| Tour de boas-vindas opcional | FALTA | Nenhum tour além do onboarding |

### 6.11 Primeiro uso

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Passos em lista vertical | FEITO | `onboarding.js` `draw` (`ol.steps`) |
| 1. Entrar / criar conta | FEITO | passos `setup` (guia de client OAuth) e `sign-in` |
| 2. Aguardando login com "Gerar link" | FEITO | passo `waiting`, botão `onboarding-generate-link` |
| 3. Apresentação | FEITO | passo `intro` |
| 4. "Veja seus arquivos no Dolphin" com botão abrir | FEITO | passo `files` (`api.openMount`) |
| 5. Escolher pastas para sincronizar (sugestões e tamanho) | FALTA | Sem passo; depende do modo Espelho |
| 6. Resumo | FALTA | Sem passo |
| 7. Como tornar disponível off-line | FEITO | passo `offline` |
| Botões Pular, Voltar, Continuar | FEITO | `footer`, `actions` em `onboarding/model.js` |
| Política `DisableOnboardingDialog` | FEITO | `gdl-common/src/config.rs:119`; `onboarding.js` `loginOnly`; `stepsFor` |


Legenda: FEITO / PARCIAL / FALTA. Caminhos relativos à raiz do repositório.

### 6.12 Integração com o gerenciador de arquivos

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Ícones de status (KOverlayIconPlugin) | FALTA | Só existe `integrations/dolphin/gdrivemenu.cpp` (menu); nenhum overlay plugin. README: "🔜 Ícones de status". |
| Abrir com Drive web | FEITO | `Action::OpenWeb`, `crates/gdl-daemon/src/menu.rs:43`. |
| Abrir com Docs / Planilhas / Apresentações | PARCIAL | Só "abrir no navegador" genérico; sem itens por app. |
| Compartilhar | FALTA | Sem ação no `menu.rs`; sem `permissions` em `gdl-drive/src/client.rs`. |
| Copiar link (com aviso público/domínio/restrito) | PARCIAL | `Action::CopyLink` copia o link; sem classificação do acesso na notificação. |
| Acesso off-line / Somente on-line (marca mista) | FEITO | `menu.rs:44-46,103-111` (`Offline::Mixed`), plugin Dolphin `gdrivemenu.cpp:93`. |
| Adicionar atalho ao Google Drive | PARCIAL | Método D-Bus `add_shortcut` existe (`dbus.rs:161`, tela "Compartilhados"), mas não está no menu do botão direito. |
| Gerenciar versões | FALTA | Sem `revisions` no cliente nem ação no menu. |
| Ver lixeira | FEITO | `Action::Trash` (`menu.rs:51,153`). |
| Atualizar pasta | FEITO | `Action::Refresh`. |
| Enviar ou sincronizar esta pasta (fora da montagem) | FALTA | Sem ação; backup/sincronização de pastas é 🔜 no README. |
| Adicionar ao espelho da conta 1–4 | FALTA | Sem modo espelho nem várias contas. |
| Gerenciar / Criar drive compartilhado | FALTA | Sem Drives compartilhados (README 🔜). |
| Criar novo documento, planilha, apresentação | FEITO | `Action::NewDocument/NewSpreadsheet/NewPresentation`, `menu.rs:47-49,67-77`. |
| Perguntar ao Gemini (web) | FALTA | Nenhuma ação no menu (só cartão Gemini na janela principal). |
| Miniaturas sem baixar (thumbnailLink) | FEITO | `crates/gdl-engine/src/thumbs/{seeder,freedesktop}.rs` (semeia o cache freedesktop com o `thumbnailLink`; não é um thumbnailer KIO próprio). |
| Excluir montagens do Baloo no primeiro uso | FALTA | `gdl-fs/src/fs.rs:47` só reconhece o processo `baloo_file` para não semear miniaturas; nada configura exclusão. |
| Outros gerenciadores: ações básicas, sem ícones | FEITO | `integrations/gtk/gdrive_desktop_linux_menu.py` (Nautilus/Nemo/Caja/Thunar), `crates/gdl-daemon/src/cosmic.rs`. |

### 6.13 Compartilhamento, versões, lixeira e criação

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Janela "Compartilhar": adicionar pessoas com papel e mensagem | FALTA | Sem tela, D-Bus ou chamada `permissions`. |
| Janela "Compartilhar": acesso geral (restrito/domínio/link) | FALTA | Idem. |
| Janela "Compartilhar": copiar link | FEITO | Pelo menu (`CopyLink`), não dentro da janela. |
| Abrir a janela pela MCP (corrigir permissão de um e-mail) | FALTA | Não há servidor MCP. |
| Gerenciar versões (baixar, restaurar, manter para sempre; mensagens) | FALTA | Sem `revisions`. |
| Lixeira: pasta somente leitura + Restaurar | FEITO (variante) | Lixeira KIO `.Trash-<uid>` na montagem (`gdl-fs/src/trash.rs`); restaurar = `engine.untrash` (`gdl-fs/src/write.rs:417-421`); 30 dias fica por conta do Drive. Não é uma pasta virtual própria com ação "Restaurar" do app. |
| Criar Docs/Planilhas/Apresentações na pasta e abrir no navegador | FEITO | `menu.rs:67-77` + `MenuAction`; `menu_action` em `gdl-cli/src/menu_action.rs`. |

### 6.14 Notificações

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Notificações do sistema com ações + lista na janela | FEITO | `gdl-daemon/src/notify.rs` (`Notifier`, `org.freedesktop.Notifications`), D-Bus `Notifications/InvokeNotificationAction` (`dbus.rs:62-74`), tela em `ui/screens/home/notifications.js`. |
| Erros fatais e de conta | PARCIAL | Existem `Offline`, `UploadFailed`; sem famílias de conta (token expirado, conta bloqueada). |
| Armazenamento e cota | PARCIAL | `QuotaFull`, `DiskLow` (`notify.rs:108,116`); faltam "quase cheio" da cota e "disco local cheio" separado. |
| Sincronização e pastas | PARCIAL | `LostAndFound`, `SavedWhilePaused`, `UploadCanceled`, `CopiedAsShortcut`; faltam local mudou, pasta conectada sumiu, itens recuperados, uploads parados há N dias, itens movidos para a lixeira. |
| Permissões e movimentações | FALTA | Nenhum evento (não pode criar/mover/excluir/renomear; avisos de compartilhamento). |
| Dispositivos (backup?, seguro remover, backup concluído/erro) | FALTA | Sem backup de dispositivos. |
| "Nova versão do app" (releases do GitHub) | FALTA | Nenhuma consulta às releases no código. |
| "Não mostrar de novo" + tela Preferências de notificação | FEITO | `dbus.rs:78-88`, `ui/screens/notification-settings.js`. |

### 6.15 Diagnóstico, feedback e Sobre

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Registro detalhado liga/desliga | FALTA | Só `tracing_subscriber::fmt()` fixo (`gdl-daemon/src/main.rs:50`); sem chave nem botão. |
| Gerar informações de diagnóstico (pacote de logs) | FALTA | Nenhum código. |
| Enviar feedback (issue pré-preenchida no GitHub + anexo) | FALTA | Nenhum código. |
| Relatório de falhas local, enviado com consentimento | FALTA | Existe só a caixa `AutomaticErrorReporting` (`ui/screens/preferences.js:176`) e a chave; nada coleta ou envia. |
| Sobre (versão, GPL-3.0, terceiros, não oficial, links) | FEITO | `ui/screens/about.js`, `gdl-ui/src/third_party.rs`, `locales/pt-BR/ui.ftl:9-12`. |
| Redefinir preferências com confirmação | FEITO | `dbus.rs:196` (`reset_preferences`), `ui.ftl:142-150`. |
| Redefinir dados locais com confirmação | PARCIAL | Só via "Desconectar conta" com apagar dados e "Desinstalar --purge" (`dbus.rs:176-181`); sem ação própria "redefinir dados locais". |

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
| DisableAddAccount | FALTA | Só lida em `config.rs:113`; sem consumidor (e sem várias contas). |
| DisableExternalMediaSync | FALTA | Sem consumidor. |
| DisableLocalizedVirtualFolders | FALTA | Sem consumidor. |
| DisableMirroredMyDrive | FALTA | Sem consumidor (sem espelho). |
| DisableMirroredFolders | FALTA | Sem consumidor. |
| DisableOnboardingDialog | FEITO | `gdl-tray/src/main.rs:148`. |
| MinFreeDiskSpaceKBytes | FEITO | `engine.rs:169`. |
| OpenOfficeFilesInDocs | FALTA | Sem consumidor. |
| DirectConnection | FEITO | `daemon.rs:262-264`. |
| TrustedRootCertsFile | FALTA | Sem consumidor (nenhum `add_root_certificate`). |
| TrustSystemRootCerts | FALTA | Sem consumidor. |
| DisableCRLCheck | FALTA | Sem consumidor. |
| DisableSSLValidation | FALTA | Sem consumidor. |
| SearchHotKey | FEITO | `gdl-tray/src/main.rs:193`, `hotkey.rs`. |
| PauseSyncingOnMeteredConnection | FEITO | `daemon.rs:398,454`, `metered.rs`. |
| AutomaticErrorReporting | PARCIAL | Chave e caixa na UI, sem funcionalidade de relatório (ver 6.15). |
| UITheme | FEITO | `ui/app.js`, `ui/screens/preferences.js`. |
| ShowMountPointInSidebar | FEITO | `daemon.rs:477-483`, `desktop.rs`. |
| PromptToBackupDevices | FALTA | Sem consumidor (sem backup de dispositivos). |

### 6.17 Servidor MCP

| Item | Situação | Evidência / o que falta |
|---|---|---|
| Binário `gdrive-desktop-linux-mcp` (stdio JSON-RPC, D-Bus) | FALTA | Nenhum crate/ocorrência de "mcp" em `crates/`, `Cargo.toml` (workspace `crates/*`) ou `packaging/`. |
| get_sync_status(path) | FALTA | Sem MCP (o D-Bus `Status` existe, mas não por caminho). |
| add_folder_to_sync(path, sync_mode, account_id?, my_drive_path?) | FALTA | Sem MCP nem backup/sincronização de pastas. |
| get_drive_url(path) | FALTA | Sem MCP (existe `web_link(item_id)` no D-Bus, `dbus.rs:152`). |
| list_tracked_folders() | FALTA | Sem MCP nem pastas conectadas. |
| open_acl_dialog(email, item_id, account_id?) | FALTA | Sem MCP nem janela Compartilhar. |
| Regra: agente nunca altera permissões em silêncio | FALTA | Não aplicável ainda (sem MCP). |

