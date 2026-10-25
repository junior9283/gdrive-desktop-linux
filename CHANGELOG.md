# Changelog

Todas as mudanças relevantes do projeto ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e o projeto usa o
[Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

## [0.1.0] - 2026-10-25

### Adicionado
- Documento de design com o catálogo de funcionalidades, a arquitetura, o roteiro e os testes
  (`docs/superpowers/specs/2026-10-25-gdrive-desktop-linux-design.md`).
- README: diferenças conhecidas em relação ao app oficial.
- Plano de implementação da versão 0.1 (motor) em `docs/superpowers/plans/`.
- Workspace Rust (8 crates), CI no GitHub Actions e mapeamento de nomes do Drive para nomes
  válidos no Linux (barra, `.`/`..`, limite de 255 bytes e nomes repetidos).
- Diretórios XDG, preferências em três camadas (política do administrador > usuário > padrão),
  com as mesmas chaves do app original e gravação atômica, e textos em pt-BR e inglês (Fluent).
- Banco SQLite por conta: metadados com revisão-base, inodes estáveis que nunca se repetem,
  intervalos já baixados, fila de envios que sobrevive a reinícios e escolha do que liberar do cache.
- Drive simulado para os testes (API v3, upload multipart e retomável, mudanças e OAuth com PKCE),
  com falhas injetáveis: limite de taxa, códigos de erro, queda de conexão e computador sem rede.
- Login OAuth no navegador com retorno ao próprio computador e PKCE, página de conclusão própria
  em pt-BR/inglês, renovação automática do acesso e guarda dos tokens no chaveiro do sistema.
- Guia para criar o próprio Client ID no Google Cloud (`docs/guia-client-id.md`).
- Cliente da Drive API v3: listagem paginada, metadados, download por intervalo, pastas, atalhos,
  lixeira, mudanças e upload multipart ou retomável (continua de onde parou após queda ou
  reinício), com recuo exponencial nas falhas passageiras e renovação do acesso após um 401.
- Indexação inicial do Meu Drive e consulta de mudanças a cada 15 s: edições remotas atualizam o
  banco e descartam o cache antigo, edições locais pendentes viram conflito em vez de serem
  sobrescritas, e Documentos Google aparecem como ponteiros `.gdoc`, `.gsheet`, `.gslides`…
- Cache do conteúdo em blocos de 1 MiB, baixado sob demanda: um salto para o meio de um vídeo
  baixa só os blocos perto do ponto lido, a leitura sequencial antecipa os próximos 8 MiB, o que já
  está no cache abre sem internet, e o cache respeita o tamanho máximo e o espaço livre mínimo
  (nunca libera itens fixados ou com edição local).
- Fila de envio que sobrevive a quedas e reinícios: arquivos novos ganham o ID do Drive mantendo o
  inode, edições sobem sem trocar o ID, envios grandes continuam de onde pararam, e sem internet a
  fila espera sem gastar tentativas. Se a nuvem mudou, apagou ou negou permissão, a edição local
  vai para `Lost and Found` com data e hora no nome, e o caminho original fica com a versão da
  nuvem. Travas e temporários de editores (`.~lock.*#`, `*.swp`…) nunca sobem.
- Disponível off-line: fixar uma pasta baixa tudo o que está dentro, inclusive o que chegar ou for
  movido para ela depois, e baixa de novo o que for editado na nuvem; o que está fixado abre sem
  internet e nunca sai do cache. Desfixar libera espaço, mas mantém o que foi fixado à parte.
- Serviço do motor com pausa: pausado, não consulta mudanças, não envia e não baixa fixados (abrir
  arquivos continua funcionando); ao retomar, volta na hora. Resumo com envios pendentes, falhas,
  pausa, conexão e hora da última sincronização.
- Montagem FUSE (leitura): a raiz mostra `Meu Drive` (ou `My Drive`), a lixeira do Drive somente
  leitura, `.Lost and Found` e `.shortcut-targets-by-id`. Listar pastas e ver tamanhos nunca baixa
  conteúdo; abrir um arquivo baixa só os trechos lidos; Documentos Google abrem como ponteiros JSON;
  atalhos do Drive viram links simbólicos; itens sem permissão de edição ficam somente leitura; sem
  internet, o que não foi baixado dá erro de E/S, mas as pastas continuam navegáveis. Atributos
  estendidos `user.gdl.id`, `user.gdl.state` e `user.gdl.web_url`; `df` mostra a cota do Drive.
- Montagem FUSE (escrita): criar, editar e truncar arquivos (o envio começa 2 s depois do
  fechamento, para temporários de editores nunca subirem; `fsync` envia na hora), salvar por
  `rename` por cima mantendo o ID do Drive, criar pastas (sem internet, sobem depois), renomear e
  mover entre pastas, data de modificação escolhida pelo app mantida (`cp -p`, `touch`), excluir
  manda para a lixeira do Drive (nunca apaga de vez), a lixeira do Dolphin (`.Trash-<uid>`) é a
  lixeira do Drive e esvaziá-la não apaga nada no Drive, copiar um ponteiro `.gdoc` cria um atalho
  para o documento e trazer de volta um arquivo do `Lost and Found` o sobe como novo.
- Serviço do usuário `gdrive-desktop-linuxd` (systemd e ativação pelo D-Bus) com a interface
  `io.github.junior9283.GDriveDesktopLinux1`: login, estado, pausar/retomar, Disponível off-line
  por caminho, Lost and Found e saída da conta (terminar envios, copiar pendentes ou descartar).
  Monta em `~/Google Drive` (ou `Google Drive (1)`… se a pasta já tiver arquivos), reconecta a
  conta na partida e respeita `AllowedAccountsPattern`.
- Notificações do sistema: edição guardada no Lost and Found (com "Ver arquivos"), sem internet,
  disco quase cheio, armazenamento do Google cheio, falha de envio e cópia de Documento Google
  transformada em atalho.
- CLI provisória `gdrive-desktop-linux` (`login`, `status`, `pin`, `unpin`, `pause`, `resume`,
  `lost-and-found`, `logout`, `open-pointer`) e abertura dos ponteiros `.gdoc`, `.gsheet`… no
  navegador pelo duplo clique (tipos MIME e atalho `.desktop`); o endereço vem do ID validado,
  nunca do campo `url` do arquivo.
- Suíte "nunca perder dados" (queda no meio do envio, rede instável, disco cheio e tempestade de
  conflitos) e guia dos testes em `docs/testes/`.

### Corrigido
- Um arquivo ou pasta criado aqui podia ser criado duas vezes no Drive quando a consulta de
  mudanças o via antes do fim do envio.
- Um arquivo ou pasta podia ser criado duas vezes no Drive quando a rede caía na hora da resposta:
  toda criação agora usa um ID reservado, e a repetição não duplica.
- Com a rede de volta, ou depois de reiniciar, os envios que esperavam a rede ficavam até 30 s
  parados; agora saem na hora.
- Revisão final da 0.1:
  - um download terminado (`.crdownload` ou `.part` renomeado para o nome final) nunca subia;
  - sair da conta apagava o Lost and Found e edições ainda sem envio; agora elas contam como
    pendentes, vão junto no `--move`, e o Lost and Found nunca é apagado;
  - sem internet, cada operação (abrir, criar pasta, enviar) insistia por 1 a 2 minutos antes de
    desistir; agora desiste em cerca de 1 s, e a fila espera a rede voltar;
  - o acesso vencido sem rede para renová-lo aparecia como falha de envio, e não como "sem
    internet";
  - gravar de novo um arquivo novo durante o envio podia criar uma segunda cópia no Drive, e um
    envio grande retomado depois de uma edição misturava a versão velha com a nova;
  - uma gravação terminada no meio de uma consulta de mudanças podia ser desfeita;
  - o próprio envio podia ser visto como conflito e deixar o arquivo preso;
  - um download em andamento podia gravar por cima de uma edição local;
  - sem limite de cache configurado, nada era liberado, e com o disco no mínimo livre toda leitura
    nova falhava; agora o que foi usado há mais tempo é liberado;
  - um erro local num envio travava a fila inteira;
  - com o armazenamento cheio, cada tentativa abria uma notificação nova;
  - esvaziar a lixeira do Dolphin com uma pasta dentro dava "permissão negada";
  - mandar para a lixeira um arquivo que ainda não tinha subido o apagava de vez;
  - um item apagado de vez pela web ficava para sempre na lixeira daqui;
  - banco, cache e Lost and Found eram legíveis por outros usuários (agora só pelo dono), e com
    `ContentCachePath` configurado a primeira leitura falhava.

## [0.0.1] - 2026-10-25

### Adicionado
- Início do projeto: README com a explicação do app, o roteiro e as regras de contribuição.
- Licença GPL-3.0.
- `.gitignore` que impede a entrada de binários de terceiros e de material de engenharia reversa.
- Este CHANGELOG e a política de versionamento.

[Não lançado]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/junior9283/gdrive-desktop-linux/releases/tag/v0.0.1
