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
```

Os testes de montagem precisam do FUSE 3 (`fusermount3`) e de acesso a `/dev/fuse`. Nenhum teste
usa a rede, uma conta Google, o barramento D-Bus da sessão ou o navegador.

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

## Armadilhas conhecidas

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
