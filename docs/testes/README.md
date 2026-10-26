# Testes

Como a suíte é organizada, como rodar, as armadilhas conhecidas e o registro dos bugs que ela já
pegou.

## Como rodar

```bash
cargo test --workspace                       # tudo (é o que a CI roda)
cargo test -p gdl-engine                     # um crate
cargo test -p gdl-daemon no_data_loss        # a suíte "nunca perder dados"
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
node --test 'crates/gdl-ui/ui/tests/*.test.js'   # a lógica das telas (sem janela)
scripts/e2e.sh                               # a janela e a bandeja de verdade (ponta a ponta)
scripts/e2e.sh search                        # só os de ponta a ponta com "search" no nome
```

Os testes de montagem precisam do FUSE 3 (`fusermount3`) e de acesso a `/dev/fuse`; os da bandeja e
da instância única, do `dbus-daemon` (cada um sobe um barramento privado). O teste de instalação
usa o `desktop-file-validate`, se houver. Nenhum teste usa a rede, uma conta Google, o barramento
D-Bus da sessão, a tela de quem roda ou o navegador.

O `scripts/e2e.sh` precisa do `WebKitWebDriver` (pacote `webkitgtk6.0` no Fedora,
`webkit2gtk-driver` no Ubuntu), do `tauri-driver` (`cargo install tauri-driver --locked`), do
`openbox`, do `xvfb-run` e do `dbus-run-session`; sem um deles, sai com 77 (pulado). Ele sobe o
serviço de teste (`crates/gdl-daemon/examples/e2e-daemon.rs`, Drive simulado com a conta já
conectada), um Xvfb e um barramento de sessão próprios e roda `crates/gdl-ui/tests/e2e.rs` em
ordem alfabética, um por vez. Nada aparece na tela.

## Onde fica cada coisa

| Crate | O que os testes cobrem | Base |
|---|---|---|
| `gdl-common` | nomes do Drive para o Linux, preferências e políticas, idiomas, tipos MIME empacotados | funções puras |
| `gdl-store` | banco SQLite: itens, inodes, intervalos baixados, fila de envios, migrações | banco temporário |
| `gdl-drive` | cliente da Drive API e OAuth: paginação, upload multipart e retomável, recuo, 401, criação idempotente | `gdl-fake-drive` |
| `gdl-engine` | motor: indexação, mudanças, cache em blocos, envios, conflitos, Lost and Found, off-line | `testutil::Env` |
| `gdl-fs` | montagem FUSE real: leitura, escrita, salvar por rename, lixeira do Dolphin, ponteiros | `testutil::Mounted` |
| `gdl-daemon` | serviço completo pelo D-Bus e a suíte "nunca perder dados" | `testutil::Harness` |
| `gdl-cli` | `open-pointer` com um navegador falso no `PATH` | binário compilado |
| `gdl-ipc` | contrato D-Bus, erros tipados e o nome único (`claim_name`); utilidades de teste (`p2p_pair`, `PrivateBus`) | barramento privado |
| `gdl-tray` | menu e ícone por estado, lançador da janela, atalho global sem portal, uma bandeja só | funções puras e barramento privado |
| `gdl-ui` (Rust) | ponte D-Bus da janela, escolha da tela pelos argumentos, idioma, uma instância só, lista de terceiros | barramento privado |
| `gdl-ui` (JS) | lógica de cada tela (`ui/screens/*/model.js`), formatação, Fluent e chaves dos textos | `node --test` |
| `gdl-ui` (e2e) | a janela e a bandeja de verdade: Início, pausar, sair, primeiro uso, preferências, Sobre, pesquisa, erros, off-line, menu da bandeja | `scripts/e2e.sh` |
| `gdl-common/tests/packaging.rs` | `install-dev.sh`: o que instala, os caminhos no `Exec`, prefixos recusados, `.desktop` válidos | prefixo temporário |

### Drive simulado (`gdl-fake-drive`)

Servidor HTTP local com a Drive API v3, o OAuth com PKCE e falhas injetáveis
(`FakeDrive::inject`):

| Falha | O que o cliente vê |
|---|---|
| `RateLimit403`, `Status`, `QuotaExceeded` | erros da API nas próximas chamadas |
| `Offline` | conexões caem até `clear_faults` |
| `DropConnectionAfterBytes(n)` | a conexão cai depois de `n` bytes |
| `StallConnectionAfterBytes(n)` | a transferência empaca depois de `n` bytes (o serviço "morre" no meio) |
| `LoseCreateResponses { times }` | a criação é feita, mas a resposta não chega |
| `DelayThumbs(d)` | cada miniatura (`/thumb`) demora `d` para responder |
| `ThumbStatus(code)` | cada miniatura (`/thumb`) responde o erro `code` da API |

Ele também conta requisições (`requests`, `requests_by_method`) e bytes baixados, que é como os
testes provam desempenho.

### Serviço de teste (`gdl-daemon::testutil::Harness`)

O serviço completo, com montagem real, sobre o Drive simulado:

- o D-Bus é ponto a ponto (um par de sockets), e o outro lado do socket também faz o papel do
  servidor de notificações;
- o "navegador" é o comando `true`, e o teste aprova o login com uma requisição HTTP;
- `restart(política)` para o serviço (saída normal, que cancela o que está em andamento) e sobe
  outro sobre os mesmos dados.

### Suíte "nunca perder dados"

`crates/gdl-daemon/src/no_data_loss_tests.rs`:

| Teste | Situação |
|---|---|
| `kill_during_upload_then_restart_finishes_upload` | o serviço para com um envio de 20 MiB empacado na metade; ao voltar, continua a mesma sessão |
| `network_flaps_during_many_writes_all_arrive` | 200 arquivos gravados com a rede caindo e voltando; todos chegam, sem cópias |
| `disk_full_never_corrupts_existing_cache` | sem espaço, o que já estava no cache continua íntegro e o que faltava dá erro (nunca dado errado) |
| `conflict_storm_preserves_every_local_version` | 20 arquivos editados aqui e na nuvem; as 20 versões locais vão para o Lost and Found e a nuvem fica intacta |

### Serviço de ponta a ponta (`examples/e2e-daemon.rs`)

O serviço de verdade sobre o Drive simulado, com a conta conectada, `notas.txt` off-line já
baixado e um "navegador" que anota em `$GDL_E2E_OPENED` o que pediram para abrir. `SIGUSR1` enche
o armazenamento (`QuotaExceeded`) e `SIGUSR2` desfaz; a confirmação vem em `$GDL_E2E_QUOTA`
(`full`/`free`), e o teste espera por ela antes de seguir.

## Armadilhas conhecidas

- **Clique em botão desabilitado no e2e se perde em silêncio:** espere o estado que habilita o
  botão (ex.: `.generate-link` só vale depois de o serviço devolver o link de login) antes do
  `click`. Uma vez só, sem reproduzir em 45 rodadas, o primeiro uso apareceu em "Entrar na conta"
  antes do clique no guia; se voltar, instrumente os eventos do `dispatch` (`window.__eventos`).
- **FUSE na CI:** é preciso instalar `fuse3` e liberar `/dev/fuse` (`chmod 666`) antes dos testes.
- **Montagens órfãs:** um teste que entra em pânico pode deixar uma montagem `gdrive-desktop-*`
  em pasta temporária. Confira com `mount | grep -c gdrive-desktop`, que deve dar 0. Para limpar
  as de nome com espaço, leia o caminho de `/proc/self/mounts` (os espaços vêm como `\040`) e
  use `fusermount3 -u -z`.
- **O serviço se referencia:** `Daemon`, a conexão D-Bus e o objeto publicado formam um ciclo. Quem
  termina chama `shutdown()` (o `Harness` faz isso no `Drop`), senão a montagem fica para trás.
- **RELEASE chega depois do close:** o kernel manda o `RELEASE` sem esperar. O registro da gravação
  acontece no `FLUSH`, que é síncrono ao `close`.
- **FLUSH de descritores duplicados:** um `fork` (inclusive o `fusermount3` de outra montagem)
  copia os descritores abertos. O `FLUSH` dessas cópias chega antes e ao mesmo tempo, por isso o
  registro é serializado por inode.
- **ENOSPC vira EIO:** pelo cache de páginas do kernel, o programa que lê recebe EIO mesmo que o
  FUSE responda ENOSPC. Teste o erro, não o número.
- **Tempo injetável:** `Intervals` e `Backoff` curtos nos testes. `UPLOAD_DELAY_MS` (2 s depois do
  fechamento) é fixo: os envios de um teste só começam depois disso.
- **Desempenho por contagem, não por relógio:** conte listagens, consultas e requisições. A máquina
  de desenvolvimento roda com carga alta, e um teste por tempo seria instável.
- **Antes de aumentar um tempo-limite,** rode o teste isolado várias vezes e também junto com o
  crate inteiro. Os testes intermitentes desta suíte eram bugs, não lentidão.
- **Xvfb sem gerenciador de janelas tira o foco ao acaso** (medido: entre 0,15 e 2,8 s depois de
  abrir, mesmo em janelas comuns). A pesquisa fecha ao perder o foco, como no original; por isso o
  `e2e.sh` sobe o openbox antes de tudo.
- **`cargo build --example` não recompila os programas:** com `-p gdl-ui -p gdl-tray --example …`
  o cargo só monta o exemplo, e o e2e rodava a bandeja velha. Programas e exemplo são compilados
  em comandos separados.
- **Texto pelo `innerText`:** o texto do elemento pelo WebDriver vem cortado com reticências
  quando o CSS corta; os testes leem o `innerText` por script.
- **`fieldset disabled`** bloqueia os controles de dentro (chave gerenciada), mas a propriedade
  `disabled` de cada um continua `false`: confira com `matches(':disabled')`.
- **O serviço de ponta a ponta é um só para todos os testes:** quem muda o estado (sair da conta,
  encher o armazenamento, pausar) desfaz no fim, mesmo em pânico (`Drop`), e cada teste prepara o
  próprio cenário (a pesquisa lê `notas.txt` para ele entrar nos recentes) em vez de contar com o
  teste anterior.
- **Sinal não é confirmação:** depois do `SIGUSR1`/`SIGUSR2`, espere o arquivo de confirmação; o
  sinal chega, mas a falha só vale quando o serviço a aplicou.
- **Instância única e ativação pelo D-Bus:** a janela ativada com `--dbus` só reserva o nome e
  espera o pedido de tela; sem isso, a ativação abriria uma tela sozinha.
- **`purge_during_seeding_leaves_nothing` é probabilístico:** ele corre o apagamento contra uma
  semeadura em andamento, então um verde não prova a ausência da corrida; falha intermitente nele
  é bug real, não azar.

## Registro de bugs

| Sintoma | Causa | Correção | Teste | Lição |
|---|---|---|---|---|
| Pasta criada aqui aparecia duas vezes no Drive | a consulta de mudanças trazia o item antes do fim do envio; `replace_id` dava UNIQUE e o envio era refeito | `Store::replace_id` descarta a cópia trazida pelas mudanças | `replace_id_drops_the_copy_that_changes_already_brought` | ID temporário e ID real convivem por um instante |
| Copiar um `.gdoc` criava dois atalhos (às vezes) | `FLUSH` de cópias do descritor, antes e ao mesmo tempo | registro serializado por inode; item já convertido é no-op | `closing_a_duplicated_pointer_descriptor_makes_one_shortcut` | todo `FLUSH` pode vir em dobro |
| Leitura logo depois do `close` vinha vazia | o `RELEASE` é assíncrono | registro no `FLUSH`; leitura usa o arquivo local enquanto há escritor | testes de escrita do `gdl-fs` | o `close` só espera o `FLUSH` |
| Com a rede de volta (ou depois de reiniciar), os envios esperavam até 30 s | o adiamento por falta de rede não era desfeito | fila marca `waiting_network`; a volta da rede e a partida liberam | `network_return_wakes_uploads_waiting_for_it`, `restart_wakes_uploads_waiting_for_network` | "tentar depois" precisa de um gatilho, não só de um relógio |
| Um envio iniciado sem rede ficava 30 s parado mesmo com a rede já de volta | a consulta via a volta antes de o envio falhar | o envio avisa a falta de rede (`report_offline`) e a próxima consulta boa libera a fila | `upload_reporting_offline_is_woken_by_the_next_good_poll`, `network_flaps_during_many_writes_all_arrive` | dois observadores da rede precisam somar o que viram |
| Arquivo ou pasta duplicado no Drive quando a rede caía na hora da resposta | a criação não era idempotente: o cliente e a fila repetiam o POST | ID reservado (`files.generateIds`) guardado no job; 409 na repetição = já criado | `create_with_reserved_id_is_not_duplicated_when_the_response_is_lost`, `lost_create_response_does_not_duplicate_the_file`, `reserved_id_survives_in_the_queue_between_attempts`, `mkdir_with_lost_response_creates_one_folder` | toda criação que pode ser repetida precisa de identidade |
| `.crdownload` renomeado para o nome final nunca subia | `move_item` só trocava o nome; o estado `LocalOnly` ficava | o nome novo decide `LocalOnly`/`Dirty` e enfileira | `crdownload_renamed_to_final_name_uploads` | o nome muda o destino do arquivo, não só o rótulo |
| Logout apagava o Lost and Found | pendente contava só a fila | conta fila, edições sem envio e Lost and Found; o Lost and Found nunca é apagado | `logout_never_deletes_lost_and_found_silently` | "pendente" é tudo o que não está no Drive |
| Sem internet, abrir ou criar pasta travava 1–2 min | `Offline` repetia pelo recuo inteiro | uma repetição rápida; quem espera a rede é a fila | `offline_gives_up_after_one_quick_retry` | teste com o recuo de produção (relógio pausado), não só com o de teste |
| Gravação local desfeita pela consulta de mudanças | ler e gravar o item em chamadas separadas | `Store::modify`: ler, decidir e gravar numa transação | `local_write_during_apply_changes_is_not_overwritten` | ler-modificar-gravar precisa ser atômico |
| Segunda cópia no Drive / envio grande misturado | tipo do envio e sessão fixados ao enfileirar | criar ou atualizar decidido na hora; gravação nova zera a sessão | `rewrite_during_create_upload_makes_one_drive_file`, `rewrite_after_interrupted_resumable_restarts_session` | o job guarda a intenção, o item guarda o estado |
| Download gravava por cima de edição local | o download não olhava o estado do item | só grava em item `Clean`, com a trava que quem edita também pega | `download_never_writes_over_a_local_edit` | dois escritores no mesmo arquivo precisam da mesma trava |
| Disco no mínimo: toda leitura nova falhava | sem limite configurado, nada era liberado | libera o acesso mais antigo antes de recusar | `low_disk_frees_the_oldest_cached_item_before_refusing` | — |
| Lixeira do Dolphin: arquivo não enviado sumia, pasta dava EACCES | lixeira só existia para o que estava no Drive | lixeira local; dentro da lixeira, apagar só esquece | `trashing_a_file_not_yet_uploaded_can_be_restored`, `emptying_kio_trash_with_folder` | o KIO apaga o conteúdo antes da pasta |
| `kill_during_upload_then_restart_finishes_upload` falhava 4 em 10 | o teste esperava `Uploading`, que vem antes dos bytes, e às vezes limpava a falha antes de o envio empacar; quando empacava, o Drive simulado segurava a trava da sessão para sempre e o envio retomado esperava | o simulado solta a trava ao ler o corpo e a requisição nova substitui a anterior (como no Drive real); o teste espera `byte_limit_reached` | `kill_during_upload_then_restart_finishes_upload` (20 de 20 isolado) | o teste passava justamente quando não exercitava o caso; espere o fato, não um estado que vem antes dele |
| A pesquisa fechava sozinha no e2e | o Xvfb sem gerenciador de janelas tira o foco ao acaso, e a pesquisa fecha no blur | openbox no `e2e.sh`; o produto continua fechando no blur | `search_finds_by_name_and_opens` | meça antes de pôr guarda no produto: o defeito era do ambiente |
| Início mostrava "Armazenamento cheio" ou "Sincronizando 1 item" depois de outro teste | o teste da cota não desfazia em pânico, e o arquivo off-line ainda baixava | `QuotaFull` desfaz no `Drop`; o serviço de teste baixa `notas.txt` e espera `syncing == 0` antes de liberar | `home_shows_synced_and_activity` | estado compartilhado entre testes precisa ser devolvido |
| O e2e da bandeja passava com a bandeja quebrada | `cargo build --example` não recompilava a bandeja | builds separados no `e2e.sh`; mutação (sem "Pesquisar") faz o teste falhar | `tray_publishes_menu` | um teste que nunca falhou não prova nada |
| Atalho do menu e início automático não achavam a bandeja fora do `PATH`; prefixo com espaço quebrava o `Exec` | `Exec` com o nome solto e caminho sem aspas | caminho completo entre aspas; o instalador recusa aspas, barra invertida, crase, `$`, `%`, `\|` e `&` | `installs_with_the_paths_of_the_prefix`, `refuses_a_prefix_that_no_exec_line_can_hold`, `autostart_file_follows_preference` | o systemd, o D-Bus e o `.desktop` separam o `Exec` nos espaços |
| Compartilhado comigo aparecia na pesquisa com caminho inexistente | `path_of` montava um caminho com qualquer cadeia de pais; o simulado escondia compartilhados que o Drive real lista | `path_of` só vale se chegar à raiz; a pesquisa filtra o que não está no Meu Drive; simulado lista compartilhados | `path_of_only_reaches_the_my_drive_root`, `search_and_recent_skip_items_outside_my_drive` | o simulado tem de errar como o Drive real, não a favor do app |
| EIO ao ler um arquivo novo no fim do envio | entre `replace_id` e o rename do cache, o banco já tinha o ID novo e o arquivo só o nome velho | hard link antes do `replace_id`; o nome velho sai depois | `read_between_id_swap_and_rename_still_finds_the_content` | troca de nome em dois lugares precisa de um instante em que os dois valem |
| Serviço não subia com `preferences.toml` quebrado | a camada do usuário era lida com `?` | camada ilegível vale como vazia, com aviso | `unreadable_user_file_does_not_stop_the_app` | arquivo que o usuário edita nunca é fatal |
| Aviso "serviço indisponível" preso; queda despercebida | a janela só sabia do serviço por chamadas que falhavam; a bandeja só relia com o estado vazio | `NameOwnerChanged` (`gdl_ipc::service_lost`) na ponte e na bandeja; chamada boa marca o serviço no ar | `service_down_gives_unavailable`, testes de `api.test.js` | o barramento avisa quando um nome some: escute |
| Conta não subia depois de mudar o cache ou a montagem | preferência de caminho só era checada pelo tipo | caminhos absolutos; cache numa pasta gravável (testado criando um arquivo) | `path_preferences_must_be_usable` | validar ao gravar o que vai ser usado na partida |
| Cancelar envio com o arquivo aberto quebrava o item | o cancelamento trocava o conteúdo com um descritor de escrita aberto | marca de escrita (`mark_writing`) do primeiro `open` ao último `release`; cancelamento devolve `in-use` | `cancel_while_open_for_writing_is_refused`, `cancel_waits_for_the_file_to_close` | o FUSE e o motor precisam se avisar do que está aberto |
| Enter abria o resultado errado na pesquisa | o Enter usava a lista da tela, ainda do texto anterior (debounce) | se a lista não é do texto do campo, pesquisa antes de abrir | `shownFor` (JS), e2e `search_enter_right_after_typing_opens_the_new_result` | lista na tela tem dono: o texto que a gerou |
| Pesquisa sem rede travava por um minuto | a pesquisa da API seguia o recuo do cliente | prazo de 2,5 s para a API (`SEARCH_API_WAIT`) | `search_does_not_wait_long_for_a_stuck_network` | chamada interativa precisa de prazo próprio; teste de prazo com limite fixo, não derivado da constante |
| "Problemas comuns" do guia mostrava `[object HTMLElement]` | `el()` achata os filhos só um nível; `map` devolvia pares `[dt, dd]` dentro de outra lista | `flatMap` | e2e `onboarding_logs_in_with_client_id` conta os `.troubles dt` | teste de unidade do modelo não vê o DOM: confira a tela (captura no Xvfb) |
| e2e do primeiro uso falhava 1 em 4 ("sem o link de login") | o clique em "Gerar link" chegava antes do link, com o botão desabilitado | o teste espera o botão habilitar | `onboarding_logs_in_with_client_id` (30 de 30) | clique em elemento desabilitado não dá erro: espere o estado |
| Janela recém-instalada: "o serviço não está respondendo" | o barramento da sessão (dbus-broker) só lê as pastas de ativação ao iniciar; a do serviço era nova | `install-dev.sh` chama `ReloadConfig` no barramento da sessão | `activation_reloads_systemd_and_the_session_bus` | instalar ativação D-Bus sem root exige reler a configuração do barramento, como o `daemon-reload` do systemd |
| Janela sem ícone na barra de tarefas e na barra de título (KDE Wayland) | o app_id da janela era o nome do programa (`gdrive-desktop-linux-ui`), sem `.desktop` com esse nome | `glib::set_prgname(APP_ID)` antes do GTK | e2e `window_class_is_the_desktop_file_id` (falha sem a correção) | o ícone vem do `.desktop` cujo nome é o app_id; confira pelo KWin (`resourceClass`/`desktopFileName`) |
| Janelas do e2e abriam na sessão real de quem roda os testes | `env -u WAYLAND_DISPLAY` não basta: a libwayland usa `wayland-0` por padrão | `GDK_BACKEND=x11` no `e2e.sh` | o mesmo e2e (o `xdotool` só vê janelas no Xvfb) | isolar a tela exige forçar o backend; confira onde a janela abriu, não só que o teste passou |
| Botão da conta só com a inicial; a foto aparecia só ao abrir o menu | a foto era buscada apenas na abertura do menu | `accountButton` busca `account_info` (cache de 1 min, uma busca em curso compartilhada) e troca a inicial pela foto | e2e `account_button_shows_photo_without_click` (falha sem a correção) | o que o usuário espera ver sempre não pode depender de um clique |
| Menu da conta fora da tela em tela cheia | `popupMenu` posicionava uma vez, com a largura inicial (só o e-mail); o cartão alargava depois | posição recalculada por `ResizeObserver` no menu | e2e `account_menu_stays_inside_window` (janela 1900 px; falha sem a correção) | conteúdo assíncrono muda o tamanho: posicionar de novo quando ele chega |
| Instalado por pacote, a janela diria "o serviço não está respondendo" até sair da sessão | o dbus-broker só lê as pastas de ativação ao iniciar, e o pacote (root) não alcança o barramento de cada usuário | `gdl_ipc::ensure_activatable`: sem o serviço entre os ativáveis, `ReloadConfig` (bandeja, janela e CLI) | sem teste automático: o `dbus-daemon` dos testes relê as pastas sozinho, então não reproduz; conferido que é no-op com o serviço já conhecido | o `postinst` roda como root; o que depende da sessão do usuário se resolve na partida do app |

## Teste manual com conta real

Roteiro da 0.1 no Nobara, com o Client ID do dono do projeto. **Situação: pendente.**

1. [ ] `gdrive-desktop-linux login` e a pasta `~/Google Drive` montada.
2. [ ] Abrir arquivos no Dolphin (miniaturas e leitura sob demanda).
3. [ ] Editar um documento no LibreOffice e salvar (salvar por rename).
4. [ ] Mover para a lixeira pelo Dolphin e conferir a lixeira do Drive.
5. [ ] `gdrive-desktop-linux pin` numa pasta e conferir o off-line.
6. [ ] Tirar o cabo de rede, editar, recolocar e ver o envio.
7. [ ] Reiniciar o computador e ver a conta reconectar.

A pasta do `rclone@GoogleDrive.service` (`~/GoogleDrive`) não conflita com `~/Google Drive`.

Roteiro da 0.2 no Plasma (Nobara). **Situação: pendente.**

1. [ ] `scripts/install-dev.sh` e "Google Drive" no menu de aplicativos.
2. [ ] Bandeja no painel do Plasma: ícone por estado, dica, menu e a animação ao sincronizar.
3. [ ] Primeiro uso com o Client ID real: login no navegador, "Gerar link" e a pasta montada.
4. [ ] Atalho global da pesquisa: o KDE pede a confirmação e o atalho abre a pesquisa.
5. [ ] Notificações com ação (Lost and Found, envio cancelado) e "não mostrar de novo".
6. [ ] Preferências: limite de banda, proxy, cache, início automático e Locais do Dolphin.
7. [ ] Abrir uma segunda bandeja: mostra a janela principal e sai.
8. [ ] Serviço parado: a bandeja mostra "Iniciar o serviço", e a janela, o aviso.
9. [ ] Reiniciar a sessão: a bandeja volta sozinha (início automático).

Roteiro da 0.3 no Plasma (Nobara). **Situação: pendente.**

1. [ ] Abrir no Dolphin uma pasta de fotos nunca aberta: as miniaturas aparecem e o cache de
   conteúdo não cresce (confira na tabela `ranges` do `metadata.db` da conta, ou no tamanho de
   `content/`, antes e depois). Sair da conta apaga as miniaturas gravadas.
2. [ ] Conferir os PNGs gravados em `~/.cache/thumbnails/{normal,large,x-large}` (um por
   tamanho, com data de agora; `xx-large` não é gravado). O nome é o MD5 da URI `file://` do
   arquivo na montagem.
3. [ ] Procurar falhas de miniatura no journal:
   `journalctl --user -u gdrive-desktop-linuxd | grep -i miniatura`. Só as da limpeza na saída
   da conta (`warn`) aparecem: o serviço registra de `info` para cima, e as de listagem,
   download e gravação são `debug`. Nenhuma mensagem pode mostrar o `thumbnailLink` inteiro
   (no máximo o host ou o ID do arquivo).
