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
integrations/dolphin/build.sh /tmp/dolphin   # o plugin do Dolphin (C++/KF6) e os testes dele
integrations/gtk/tests/run.sh                # a extensão do Nautilus, Nemo, Caja e Thunar
# as imagens do README, geradas no Xvfb com o Drive simulado
GDL_SCREENSHOTS=docs/imagens scripts/e2e.sh screenshots
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

O plugin do Dolphin (`integrations/dolphin`) é testado pelo `tests/run.sh`: o `probe` monta o menu do
botão direito como o Dolphin (`KFileItemActions`, Qt sem tela) contra um serviço falso em Python
(`fake_service.py`) num barramento próprio. A regra de quais itens aparecem é testada no serviço
(`menu.rs` e `dbus_tests.rs`); o plugin só desenha a resposta. A extensão GTK usa o mesmo serviço
falso, com módulos falsos no lugar do Nautilus, Nemo, Caja e Thunar (não precisam estar instalados).
As ações do COSMIC Files são testadas em `cosmic.rs` (preserva as do usuário, não duplica, sai na
desinstalação, não mexe em arquivo ilegível).

## Onde fica cada coisa

| Crate | O que os testes cobrem | Base |
|---|---|---|
| `gdl-common` | nomes do Drive para o Linux, preferências e políticas, idiomas, tipos MIME empacotados | funções puras |
| `gdl-store` | banco SQLite: itens, inodes, intervalos baixados, fila de envios, migrações | banco temporário |
| `gdl-drive` | cliente da Drive API e OAuth: paginação, upload multipart e retomável, recuo, 401, criação idempotente | `gdl-fake-drive` |
| `gdl-engine` | motor: indexação, mudanças, cache em blocos, envios, conflitos, Lost and Found, off-line | `testutil::Env` |
| `gdl-fs` | montagem FUSE real: leitura, escrita, salvar por rename, lixeira do Dolphin, ponteiros | `testutil::Mounted` |
| `gdl-daemon` | serviço completo pelo D-Bus (inclusive o menu do botão direito) e a suíte "nunca perder dados" | `testutil::Harness` |
| `integrations/dolphin` | plugin do menu do Dolphin: itens, marcas, área de transferência, erros | `tests/run.sh` |
| `integrations/gtk` | extensão do Nautilus, Nemo, Caja e Thunar: gerenciador pela pasta, submenu, separadores, ações e erros | módulos falsos de cada gerenciador e `tests/run.sh` |
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
| `DelayUploads(d)` | cada envio de conteúdo (`/upload/…`) demora `d`; `max_concurrent_uploads` conta os simultâneos |
| `DelayCreates(d)` | cada criação na API (POST fora de `/upload/…`, como a de uma pasta) demora `d` |
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
- **Limite de bytes do teste de saltos depende do kernel:** `seeks_download_little_more_than_what_is_read`
  conta o que o kernel adianta na leitura (read-ahead), que varia entre máquinas. O limite era 16
  MiB e o CI da tag `v1.2.2` baixou 19 MiB uma vez (o mesmo código passou na `main` e aqui, 12 de 12);
  subiu para 24 MiB, que ainda pega a regressão de verdade (32 e 62 MiB), mas deixa passar uma
  entre 16 e 24 MiB. Se aparecer um teste que precise de menos folga, meça em algumas execuções do
  CI em vez de apertar o número, ou conte requisições ao Drive, que dependem menos do kernel.
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
- **Plugins do Dolphin nos testes:** as sondas rodam com Qt `offscreen` num barramento próprio; o
  serviço falso (`integrations/testing/fake_service.py`) emite sinais pelo PyGObject, porque o
  `gdbus` não existe no contêiner Debian da CI. A espera da sonda é de 3 s (1 s falhava com a
  máquina ocupada).
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
| Desinstalado pela loja/apt, a sessão tentaria abrir a bandeja apagada a cada login | o gerenciador roda como root e não alcança o `~/.config/autostart` do usuário | `TryExec` no `.desktop` do início automático | `autostart_file_follows_preference` | o que fica na pasta do usuário precisa se desligar sozinho quando o programa some |
| `gdrive-desktop-linuxd --version` montava "~/Google Drive (1)" e reiniciava o serviço | o binário não lia opções, e o `Builder::name` do zbus pede o nome com `AllowReplacement \| ReplaceExisting` | opções tratadas antes de tudo; nome pedido com `gdl_ipc::claim_name` (só `DoNotQueue`), e a segunda instância sai | `tests/cli.rs` (casa vazia, sem barramento); `claim_name` coberto pela bandeja | padrão de biblioteca também é decisão: confira as flags de quem pede um nome |
| CPU parada em ~2,5 % com um Drive de 610 mil itens | consulta periódica com `ORDER BY` e parâmetro no lugar do literal (`pin != ?1`) não usava o índice parcial | literais nas condições dos índices parciais, ordenação em Rust, `UNION` no lugar de `OR`; índice `items_edited` | `periodic_queries_do_not_scan_every_item` (passos de varredura e plano) | desempenho se prova contando passos, não medindo tempo |
| Download cancelado no meio poderia prender o trecho reservado | a reserva de trecho em download precisa sair também no cancelamento | `Fetching` solta a reserva no `Drop` | `canceled_download_releases_its_range` (falha com a liberação desligada) | toda reserva em memória precisa de dono que a solte em qualquer saída |
| 8 saltos de 256 KiB num vídeo baixavam 62 MiB (eram 32 MiB) | o kernel transforma cada salto em 3 leituras seguidas; a 2ª já disparava 8 MiB adiante, e a janela dobrada disparava mais 16 MiB na 3ª | leitura antecipada só depois de 1 MiB lido em sequência; a próxima janela sai na metade da última pedida | `seeks_download_little_more_than_what_is_read` (montagem real), `sequential_read_prefetches_8_mib_after_a_block` | medir saltos junto com a velocidade sequencial: otimizar um piora o outro |
| Cancelar o envio às vezes tirava o conteúdo de um arquivo aberto para escrita | a marca "aberto para escrita" era um conjunto: o último fechamento de uma abertura anterior (o `RELEASE`, que o kernel manda sem esperar) chegava depois da nova abertura e apagava a marca dela | marcas contadas por inode (`WritingMark`) | `late_release_keeps_the_mark_of_a_new_open`; achado por `cancel_waits_for_the_file_to_close`, que falhava ~1 em 6 rodadas da suíte inteira | marca compartilhada por várias aberturas precisa de contagem, não de presença |
| Leitura sequencial fazia um pedido a mais ao Drive com a máquina ocupada (8 em vez de 7 para 64 MiB) | a leitura antecipada reservava o trecho dentro da tarefa (`tokio::spawn`); a leitura seguinte chegava antes e pedia o bloco à parte | o primeiro trecho é reservado na hora, antes da tarefa | `sequential_read_uses_growing_windows` (0 falhas em 4 rodadas da suíte inteira; antes, 1 em 2) | o que precisa estar valendo quando a função volta não pode ficar para uma tarefa |
| `disabled_kind_is_neither_shown_nor_listed` falhava às vezes (aviso da Copia1 aparecia) | o aviso desligado é decidido quando o serviço trata o evento; religar logo depois da cópia chegava antes do tratamento | barreira no teste: um aviso ligado gerado depois (gravação com a sincronização pausada), tratado em ordem | o mesmo (15 de 15 isolado) | "a operação voltou" não quer dizer "o evento dela foi tratado": espere um sinal que venha depois |
| Trocar o acesso geral podia deixar o de outro tipo (domínio e link juntos) | a troca saía cedo ao achar o primeiro acesso geral | tira todos os de outro tipo antes; o Drive simulado aceita os dois, como o real | `changing_general_access_removes_every_other_kind` | o simulado não pode simplificar o que o Drive real permite |
| Ler um arquivo aberto enquanto ele subia dava erro de E/S depois do fim do envio | o handle de leitura guardava o ID temporário; ao fim do envio o conteúdo e a linha do banco passam para o ID do Drive e o nome velho some | o handle guarda também o inode e, se a leitura falha com o ID tendo mudado, passa para o novo | `open_file_still_reads_after_the_upload_finishes`; achado por `create_write_close_uploads_new_file`, que falhava ~1 em 40 rodadas (também na 0.7) | ID que muda ao longo da vida do item não serve de identidade guardada: guarde o inode |
| `UNIQUE constraint failed` ao mudar o estado de um item | `INSERT OR IGNORE` no gatilho herda a política de conflito do comando de fora (o upsert) | `ON CONFLICT DO NOTHING` no gatilho | testes de `take_state_changes` | em gatilho do SQLite, `OR IGNORE` não vale: a cláusula de conflito de fora prevalece |
| Um arquivo só aparecia como "2 envios pendentes" (às vezes, em `expired_account_does_not_stop_the_others`) | cada gravação fechada enfileira mais um envio, e `queue_stats` contava linhas da fila (`COUNT(*)`) | conta itens distintos (`COUNT(DISTINCT item_id)`) | `pending_counts_files_not_jobs` | número para a pessoa conta arquivos, não tarefas internas |
| `network_flaps_during_many_writes_all_arrive` falhava ~1 em 25-40 rodadas com a CPU carregada (também na 0.7) | o teste cobrava vazão (metade dos 200 em 30 s com a rede caindo a cada 300 ms); com a rede estável o motor voltava na hora (medido) | as oscilações duram até metade subir ou 30 s, sem falhar; depois todos têm de chegar (até 60 s), sem cópias e íntegros | o próprio teste, 0 em 30 sob carga; a fila parada segue coberta por `upload_reporting_offline_is_woken_by_the_next_good_poll` | antes de culpar o código, compare com a versão anterior sob a mesma carga e instrumente o que estava parado |
| Com Drives compartilhados, a consulta de mudanças quebraria a cada drive criado, renomeado ou tirado da conta, e a conta pararia de sincronizar | `changes.list` traz entradas `changeType: drive` sem `fileId`, e o campo era obrigatório; o simulado não mandava esse tipo | `fileId` opcional, `Change::Drive` consulta a lista de Drives (nome novo, drive novo, drive que saiu) | `drive_changes_come_without_a_file`, `renamed_drive_gets_the_new_name_on_the_next_poll`, `a_drive_taken_away_leaves_the_mount` | o simulado tem de mandar tudo o que a API real manda, inclusive os tipos que o app ignora |
| Um Drive compartilhado ilegível (403/404 na listagem) impedia a conta de montar e, depois, de sincronizar | `sync_drive_list` propagava o erro de um drive | o drive que falha fica de fora até a próxima consulta | `a_drive_that_cannot_be_read_does_not_stop_the_rest` | numa lista de coisas independentes, a falha de uma não pode parar as outras |
| Desligar um drive podia esquecer uma gravação feita enquanto outro drive indexava | conferir pendências e esquecer eram passos separados, com rede no meio | `Store::disable_drive` confere e esquece na mesma transação | `disabling_a_drive_checks_and_forgets_at_once` | checar e agir sobre o banco na mesma transação |
| Conta que não subiu na partida sumia do arquivo `accounts` na gravação seguinte; a segunda conta mudava de pasta se a primeira saísse; pausa não valia para conta adicionada depois; a bandeja contava só a primeira conta | a lista em memória era a única fonte do arquivo; "primária" era a posição no arquivo; a pausa era aplicada só às contas existentes; o `Status` sem conta usava os números da primeira | contas que não subiram ficam guardadas; `\tbeside` no arquivo; pausa do usuário guardada no serviço e aplicada na montagem; totais somados | `an_account_that_did_not_start_stays_saved`, `the_second_account_keeps_its_folder_after_the_first_leaves`, `pause_reaches_an_account_added_later`, `status_of_every_account_adds_the_counts_up` | ao passar de um para muitos, revise cada lugar que assumia "a conta" |
| Quem atualizasse da 0.7 não veria os Drives compartilhados (achado ao instalar a 0.8 na sessão real) | a lista de Drives só era consultada na primeira indexação, numa mudança de drive ou ao abrir "Gerenciar"; o banco da 0.7 já estava indexado | a primeira consulta de mudanças da partida consulta a lista também; se falhar, só vai para o log e é tentada de novo | `the_first_poll_lists_the_drives_of_an_account_indexed_before_them`, `the_first_poll_applies_the_changes_even_without_the_drive_list` | teste a atualização a partir do banco da versão anterior, não só a instalação nova |
| Sair pela bandeja e abrir de novo pelo menu levava ao primeiro uso, com a conta conectada (relatado pelo usuário; também na 0.6) | o serviço publica o nome no barramento e reconecta as contas guardadas em paralelo; a bandeja perguntava o estado antes de a conta subir e recebia `signed-out` | estado `starting` ("Conectando à sua conta…") enquanto há contas guardadas por reconectar; no fim, `StatusChanged`, e só sem nenhuma de pé vira `signed-out` | `status_says_starting_until_the_saved_accounts_reconnect`, `when_no_saved_account_comes_back_status_says_signed_out`, `starting_service_does_not_ask_to_sign_in`, teste do cabeçalho em `home.model.test.js` | "ainda não sei" não é "não tem": estado de partida precisa de nome próprio |
| `many_new_files_upload_in_parallel_after_their_folder` falhava às vezes ("1 envios juntos"); com rede lenta, uma pasta nova subia os arquivos um por vez | os quatro envios do `upload_loop` rodam juntos; os que só achavam arquivos esperando a pasta-mãe (adiados por `PARENT_WAIT_MS`) terminavam, e se a pasta demorasse mais que a espera, o que a criou subia o resto sozinho | um envio sem trabalho livre espera enquanto outro da equipe (`Crew`) ainda envia, e tenta de novo quando ele termina | `slow_folder_creation_still_uploads_its_files_in_parallel` (`Fault::DelayCreates`; falha sempre sem a correção); 30 de 30 isolado e 15 de 15 no crate sob carga antes de reproduzir | quem trabalha em equipe não sai só porque agora não há nada: o que um termina pode liberar trabalho para os outros |
| Senha de proxy com `:` ou `@`, ou usuário sem senha, entrava no pacote de diagnóstico | a regra de credencial no endereço exigia `usuario:senha@` sem `:`/`@` na senha | a regra corta tudo entre `://` e o último `@` antes da `/` | `diagnostics_scrub_tokens_and_emails` (casos `p@ss:w0rd` e `soutoken@`) | filtro de segredo testado com os caracteres que a senha pode ter |
| Relatórios de falha legíveis por outros usuários da máquina | a pasta `crashes` e os arquivos eram criados com as permissões padrão (umask) | `create_private_dir` (0700) e o arquivo em 0600 | `panic_writes_a_report_and_keeps_twenty` | todo arquivo que pode ter caminho, nome ou e-mail nasce privado |
| Pelo MCP, um assistente (ou um texto que o engane, por injeção de prompt) podia levar ao Drive `~/.ssh`, `~/.gnupg` ou qualquer pasta (achado pela revisão de segurança automática do commit) | `add_folder_to_sync` aceitava qualquer pasta que o serviço aceitaria pela interface | o servidor MCP só aceita pastas visíveis dentro da pasta pessoal, seguindo links, nos dois modos | `agent_cannot_add_hidden_or_outside_folders` (falha com a checagem desligada) | o que um agente pode pedir sozinho precisa de limites mais estreitos que o que a pessoa faz na tela |
| Banco da conta estragado na tabela `items` (ou ilegível): o conteúdo das edições que não subiram era apagado (revisão final da 0.10) | a recuperação copiava só as edições lidas e depois apagava o cache inteiro; com a leitura interrompida, as não lidas sumiam | `edited_items` diz se leu tudo; sem isso, o cache vai para `content-recuperacao-<data>/` em vez de ser apagado; nomes repetidos são numerados e IDs que não servem de nome são ignorados | `damaged_items_table_keeps_the_cache`, `unreadable_file_keeps_the_cache`, `same_path_edits_are_both_kept_and_bad_ids_skipped` | o teste de recuperação precisa estragar a tabela que importa, não uma vizinha; apagar dado do usuário só com a lista completa do que é dele |
| Depois do aviso de atividade suspeita, reiniciar retomava os envios cifrados; "Eu reconheço" desfazia também a pausa do usuário e a da conexão limitada (revisão final da 0.10) | o motivo `suspicious` ficava só na memória; `recognize_activity` chamava `resume()`, que zera todos os motivos | o motivo é gravado no kv da conta e lido na abertura; reconhecer desfaz só ele | `suspicious_pause_survives_restart` | estado que espera uma decisão do usuário precisa sobreviver a reinício; cada ação desfaz só o próprio motivo |
| Duas chamadas `MoveToDrive` para a mesma pasta: o desfazer da segunda apagava a cópia da primeira (revisão final da 0.10) | a conferência de "já existe" vinha antes da criação do destino, e o desfazer apagava o destino mesmo sem tê-lo criado | o destino é criado (atomicamente) antes do trecho que desfaz: só desfaz quem criou | `swap_leaves_a_target_it_did_not_create` | quem desfaz só desfaz o que criou; conferir e criar são um passo só (`create_dir`) |
| "Redefinir os dados locais" sem rede tirava a conta da lista e perdia as pastas do computador (revisão final da 0.10) | o banco era apagado antes de a conta subir de novo, e subir precisa do Drive | antes de apagar, o serviço pergunta ao Drive (`about`); sem resposta, recusa com aviso | `reset_local_data_refuses_offline` | operação que apaga e reconstrói confere antes as dependências da reconstrução |
| O exemplo `e2e-daemon` não compilava (faltava `updates` na `Config`) e a suíte e2e não rodou desde a checagem de versões | a `Config` ganhou um campo e só o harness dos testes de unidade foi atualizado | o exemplo usa o Drive falso, com a primeira consulta depois de uma hora | suíte e2e (`scripts/e2e.sh`) | campo novo na `Config`: procurar todos os `Config {` do workspace, exemplos incluídos (`cargo build --examples`) |
| `states_inside_a_pair` passou a falhar sempre na suíte do daemon (`["offline", "syncing", …]`) depois da coluna `owned_by_me`; isolado, passava | o teste esperava o arquivo aparecer no Drive simulado e já conferia o estado; o par só anota o arquivo como sincronizado depois de gravar o item (`remember`, depois `put_synced`), e qualquer atraso nesse intervalo aparece | o teste espera o estado `offline` do arquivo (até 5 s) antes de conferir | `states_inside_a_pair` (3 de 3 na suíte inteira; o estado chega, então não era regressão) | esperar pelo sinal externo (o Drive) e conferir o estado interno é corrida: espere pela condição que o teste confere |
| `errors_screen_points_to_the_other_account` falhava na suíte e2e completa ("esperando: a segunda conta"); isolado, passava | no Drive simulado, `login_as` vale para um login só, e `account_menu_switches_and_adds_accounts`, que roda antes, já o tinha gastado: o segundo login voltava para a conta principal | o app manda `login_hint` com a conta esperada (o Google pré-seleciona a conta, também no "Entrar de novo"); o simulado honra o `login_hint` depois da escolha explícita, e o teste pede a conta | `auth_url_hints_the_expected_account`, `login_hint_picks_the_account`, e2e na ordem da suíte | estado de uso único no simulado faz um teste depender da ordem: rodar a dupla junta antes de dar o teste como pronto |
| `account_menu_switches_and_adds_accounts` falhou uma vez na suíte e2e com "a janela nova não apareceu" | intermitente: 3/3 isolado e a suíte seguinte inteira verde, sem mudança no código da janela | nenhuma por enquanto | o próprio teste | se voltar, medir quanto a janela nova demora no Xvfb antes de aumentar a espera |
| "Mover tudo para…" (revisão final da 0.11): com o destino em outro disco, se a origem não saísse inteira (subpasta sem permissão), a cópia completa era apagada e o que já tinha saído da origem sumia; links simbólicos eram pulados na cópia e apagados da origem | a limpeza do destino valia para qualquer erro, também depois da cópia inteira; a cópia era a do MCP, que pula links | o destino só sai se a cópia não terminou; a cópia do Lost and Found leva os links e recusa arquivos especiais | `copy_then_remove_keeps_the_copy_when_the_source_resists` | numa troca que copia e apaga, depois da cópia inteira o destino nunca é desfeito |
| Tour sem conta: o balão do passo "navegação" aparecia no canto, sem destacar nada (revisão final da 0.11) | o tour pulava só alvos inexistentes; sem conta a navegação existe, mas escondida (`hidden`) | conta como na tela só o que tem caixa (`getClientRects`) | `um alvo escondido conta como fora da tela` (node) | "existe no DOM" não é "aparece": conferir a caixa |
| `relocating_to_another_folder_merges_and_trashes_nothing` falhava às vezes na suíte ("a.txt baixado na pasta nova"; 0.11, 1.0 e o CI do `main` da 1.0.0) | trocar a pasta de lugar só cancelava o laço do par; a passada em andamento terminava depois e regravava o estado sincronizado que `set_pair_local` tinha apagado, e o laço novo via `a.txt` sumido da pasta nova (exclusão local) em vez de baixá-lo | `Runners::stop` devolve a tarefa do laço; a troca espera ela terminar antes de gravar o lugar novo e subir o laço novo (em segundo plano, sem segurar a resposta D-Bus) | o próprio teste (2 falhas em 8 rodadas antes; 0 em 8 depois) | cancelar não é parar: quem reaproveita o estado de uma tarefa espera ela terminar |
| `stream_move_undoes_on_failure` (MCP) falhou na suíte ("a cópia pela metade saiu"; 1.0 e 1.1, cerca de 1 em 20 rodadas, só com a máquina carregada) | o desfazer (`remove_dir_all` na montagem) recebia `ENOTEMPTY`: uma consulta de mudanças lida antes de a subpasta ir para a lixeira chegava depois e a tirava de lá; `apply_changes` aplicava qualquer resposta sem ordem | o app pede a `version` de cada item ao Drive, guarda a última aplicada (coluna `items.version`) e descarta a resposta mais antiga (1.1) | `stale_upsert_does_not_untrash` (determinístico), o próprio teste | "esperar a condição" não serve quando a condição nunca chega: a falha de um desfazer que não repete é do produto; instrumentar (`eprintln` do erro ignorado) mostrou a causa em uma rodada |
| `mute_action_turns_the_kind_off` falhou no CI do `main` da 1.0.0 (`!resume.enabled`) | o teste clicava no aviso assim que o servidor de notificações simulado o recebia; o serviço só guarda no histórico o ID dado pelo servidor quando o `Notify` responde, e um clique antes disso não acha o aviso | uma trava (`showing`) segura do envio do aviso até o ID estar no histórico, e o clique a pega antes de procurar; os testes voltaram a clicar direto | `mute_action_turns_the_kind_off`, `encrypted_burst_pauses_and_can_be_recognized`, testes de dispositivo | corrida achada no teste vale também em produção: corrigir no serviço, não só esperar no teste |
| Revisão final da 1.1: com um terminal parado dentro da pasta do Google Drive, mudar o cache de lugar deixava as contas desmontadas para sempre (e travava sair da conta e login) | a Tarefa 7 trocou a espera pelos arquivos abertos pela espera da sessão FUSE antiga; com desmontagem lazy, a sessão só acaba quando sai a última referência, e um diretório atual conta | a reabertura voltou a esperar só os arquivos abertos (sem prazo) e o motor antigo (prazo de 10 s); a sessão antiga segue sozinha | `restart_does_not_wait_for_a_shell_inside_the_mount`, `restart_waits_for_open_files` | o sinal "terminou" de uma montagem lazy depende de quem está dentro dela, não só de arquivo aberto: esperar o que importa (as gravações), com prazo para o resto |
| Com um programa preso segurando um arquivo da pasta do Google Drive, mudar o cache de lugar, sair da conta ou desinstalar esperava sem fim (e travava o login) | a espera pelos arquivos abertos não tinha prazo e cada caminho (reabrir, sair, remover) esperava de um jeito | um `settle` só, com prazo (`Config::unmount_limit`, 60 s) e aviso no log do que ficou; sair/redefinir/desinstalar esperam os arquivos antes de apagar o cache | `restart_gives_up_waiting_after_the_limit`, `sign_out_waits_for_open_files_then_gives_up`, `sign_out_does_not_wait_when_nothing_is_open` | toda espera por algo fora do nosso controle tem prazo e deixa o motivo no log |
| Trocar de lugar uma pasta do computador com um envio preso deixava a troca (e a trava de "uma troca por vez") presas para sempre | a tarefa da troca esperava a passada antiga sem prazo e sem olhar se a conta parou | `select!` entre a passada antiga, a parada da conta e o prazo; o par fica no lugar antigo | `relocation_gives_up_when_the_old_pass_never_ends` | quem espera outra tarefa também escuta o cancelamento e o relógio |
| Um servidor de notificações lento atrasava os avisos seguintes e o clique em qualquer um | a trava `showing` ficava presa durante o envio de cada aviso | contador de avisos a caminho (`watch`) que o clique espera por no máximo 2 s; os envios não se esperam | `slow_notification_does_not_block_the_next_one`, `click_waits_at_most_two_seconds_for_a_slow_server` | uma trava segurada durante uma chamada externa serializa o que não precisa |
| Arquivo ilegível numa pasta do computador aparecia como "Outros erros" | o texto de "sem permissão" fala do Lost and Found, e a categoria era escondida em `unknown` | categoria própria `local-no-permission` com texto próprio | `folder_failures_never_speak_of_lost_and_found`, `errors.model.test.js` | esconder a categoria perde informação; um texto que não serve pede uma categoria, não um disfarce |
| A desmontagem da pasta ao largar a conta dependia da ordem dos campos de `Account` | nada protegia a ordem | comentário na struct e teste que olha `/proc/self/mountinfo` | `signing_out_unmounts_the_drive_folder` | ordem de queda implícita merece um teste, mesmo sem um `Drop` explícito |
| Um item apagado no Drive podia reaparecer no banco (e na pasta) quando a resposta de uma operação da montagem (renomear, mover, lixeira), lida antes da remoção, era aplicada depois | a resposta era gravada como um item novo quando a linha já não existia, e o Drive não manda `version` na remoção para a guarda de versão comparar | operações sobre item conhecido gravam com `apply_known`/`applied_known`: sem a linha, não recriam (`NotFound`); só a consulta periódica e as criações inserem | `operation_response_does_not_recreate_a_removed_item`, `operation_response_updates_a_known_item` | a corrida estava na origem da resposta (a operação), não no relógio: nada de lápide com prazo, que esconderia o item que volta com a mesma versão |
| O ícone da bandeja ficava animado ("sincronizando") para sempre depois de o último envio ou download terminar (relatado pelo usuário) | a bandeja só relê o estado com `StatusChanged`, e o serviço só o emitia em pausa, offline, login e algumas ações: o fim da fila de envios e dos fixados não passava por evento | o laço periódico dos estados (`forward_states`) compara o resumo mostrado (sem hora da última sincronização e estimativa) e emite `StatusChanged` quando muda | `status_changed_is_signaled_when_the_last_upload_ends` (uma "bandeja" que só relê por sinal) | quem só atualiza por sinal exige que todo caminho que muda o estado o emita; o teste tem de imitar o consumidor, não só ler o estado |
| Um lote de movimentações (`mv` de muitos arquivos, feito por outra sessão) deixava as pastas envolvidas sem responder por vários segundos no Dolphin, vazias ou com cadeado, até fechar e abrir de novo (relatado pelo usuário; reproduzido na pasta dele: 16 `mv` paralelos, listagem de até 9,6 s e `ENOENT` em arquivos recém-listados) | cada `rename` esperava a rede (~1 s) antes de voltar, e o kernel segura as duas pastas durante o `rename` entre pastas: as movimentações andavam uma de cada vez e quem listava essas pastas ficava na fila | o lugar novo vale no banco na hora e o Drive é avisado em segundo plano (`moves.rs`); o pedido fica na tabela `pending_moves` até o Drive confirmar, nenhuma resposta dele o desfaz antes (`apply_changes`), recusado ele volta e avisa, e a partida retoma; só para item editável já no Drive | `a_batch_of_moves_does_not_stall_the_folders` (6,6 s antes, <2 s depois), `move_returns_before_the_drive_answers`, `poll_does_not_undo_a_pending_move`, `a_second_move_while_pending_wins`, `refused_move_goes_back`, `a_passing_error_is_retried_not_undone`, `pending_move_resumes_after_a_restart`, `a_refusal_never_drops_a_newer_request`, `a_synchronous_path_waits_for_the_pending_move` (o caminho que fala com o Drive na hora, como o que avisa exposição, espera o pedido pendente do mesmo item; `a_move_with_a_warning_after_a_pending_move_is_not_undone` cobre a combinação, mas não o entrelaçamento) | operação que o kernel faz com a pasta travada não pode esperar a rede; e o que se mostra antes de confirmar precisa estar no banco, para sobreviver a uma queda |
| Revisão final da 1.1: o "Retomar" do aviso "salvo durante a pausa" desfazia a pausa por atividade suspeita da própria conta | os dois avisos usavam a mesma chave `resume`, e ela chamava `resume()`, que zera todos os motivos | a chave `allow-sync` é só do aviso de atividade suspeita; `resume` mantém a suspeita | `resume_from_another_note_keeps_the_suspicious_pause` | ação que desfaz uma proteção tem chave própria, nunca a genérica |
| `first_listing_of_a_photo_folder_has_the_thumbnails` falhou 2 vezes em 22 rodadas da suíte do daemon (1.0.0) | a listagem espera as miniaturas só até `THUMB_WAIT` (1,5 s) e segue sem elas; com a máquina carregada, o prazo vencia e o teste exigia as 6 gravadas | o teste aceita as 6 gravadas ou a listagem ter esperado o prazo inteiro; nos dois casos, nenhum byte de foto baixado (1.1) | o próprio teste | teste de garantia com prazo confere o prazo, não só o caso rápido |

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
