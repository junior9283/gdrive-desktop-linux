# Changelog

Todas as mudanças relevantes do projeto ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e o projeto usa o
[Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

## [1.2.0] - 2026-10-29

Robustez: nada de espera sem fim ao desmontar, e avisos e erros que dizem o que aconteceu.

### Corrigido
- Mudar o cache de lugar (ou sair da conta) com um programa preso segurando um arquivo da pasta do
  Google Drive não trava mais: o app espera até 60 s a pasta largar os arquivos e segue, deixando
  o motivo no log. Antes, a espera não tinha fim e travava também o login.
- Sair da conta, redefinir os dados locais e desinstalar esperam a pasta largar os arquivos antes
  de apagar o cache, para não tirá-lo de debaixo de um programa que ainda grava.
- Trocar de lugar uma pasta do computador com um envio preso desiste no prazo, e também ao parar a
  conta, e a pasta continua no lugar antigo; antes, a troca e a trava de "uma troca por vez" ficavam
  presas para sempre.
- Um servidor de notificações lento não atrasa mais os avisos seguintes, e o clique num aviso
  espera os que estão a caminho por no máximo 2 s.
- Janela de erros: arquivo que o app não consegue ler numa pasta do computador aparece em
  "Sem permissão para ler no computador", com texto próprio, em vez de "Outros erros".

## [1.1.0] - 2026-10-28

Pendências da 1.0: correções de corridas raras e o emblema de erro nas pastas do computador.

### Adicionado
- Pastas do computador: o arquivo que não sincronizou ganha o emblema de erro no gerenciador de
  arquivos, como na pasta do Google Drive.
- O arquivo de uma pasta do computador que não sincronizou aparece na janela de erros, com o
  motivo; "Tentar de novo" compara a pasta na hora e "Abrir pasta" mostra o arquivo. O contador
  de erros da bandeja também o conta.

### Corrigido
- Trocar de lugar uma pasta do computador às vezes deixava de baixar os arquivos dela na pasta
  nova: a troca agora espera a comparação em andamento terminar. Uma segunda troca da mesma pasta
  antes de a primeira acabar é recusada ("Esta pasta ainda está mudando de lugar").
- "Retomar" numa conta desfazia a pausa por atividade suspeita das outras contas; agora só a
  confirmação nelas a desfaz.
- O emblema de um arquivo novo podia ficar parado em "sincronizando" depois do envio.
- Sair da conta ou redefinir os dados locais durante a reabertura das contas podia montar a conta
  de novo; agora esperam a reabertura terminar.
- Clicar num aviso logo que ele aparece às vezes não fazia nada.
- O "Retomar" do aviso "salvo durante a pausa" desfazia a pausa por atividade suspeita da própria
  conta; agora só "Permitir sincronização", no aviso da atividade suspeita, a desfaz.
- Um item recém-enviado para a lixeira pela pasta do Google Drive podia voltar por um instante
  (e `rm -r` falhar com "diretório não vazio"), quando uma consulta ao Drive feita antes chegava
  depois: o app agora usa a versão de cada item no Drive e descarta a resposta mais antiga.

## [1.0.0] - 2026-10-28

Primeira versão estável: cada item da [matriz de paridade](docs/paridade.md) com a versão Windows
está feito, ou registrado em "Diferenças inevitáveis" no README.

### Adicionado
- Lixeira do Drive: arrastar um item da `.Lixeira do Drive` para uma pasta do Drive o restaura; um
  nome repetido não substitui nada (os dois ficam, numerados).
- Emblema de erro no Dolphin: o arquivo que está na lista de erros aparece com "!" e volta ao
  normal quando o envio dá certo.
- Preferências › Google Drive: "Alterar" muda a pasta da conta mostrada (vale ao reiniciar).
- Mudar a pasta do cache não pede mais para reiniciar: o app para as contas, leva o cache com o
  aviso em porcentagem e as reabre.
- Atividade suspeita: renomear muitos arquivos para extensões de arquivo cifrado (`.locked`,
  `.encrypted`…) também pausa os envios.
- Avisos com "Não mostrar de novo" no próprio aviso e na lista da janela; o aviso de dispositivo
  traz "Sempre fazer backup" e "Ignorar".
- Aviso ao mover algo para fora de um Drive compartilhado.
- Aba "Meu computador": sugestões de pastas para adicionar com um clique.
- Login: um link para criar uma conta Google no passo de entrar.
- Gerenciar versões diz quando o arquivo não tem versões e quando a lista não carregou, com
  "Tentar de novo".
- "Gerenciar drives compartilhados" escolhe a conta quando há mais de uma.

### Mudado
- "Somente on-line" libera o espaço na hora (sem tocar no que está aberto ou ainda não subiu).
- "Drives compartilhados" só aparece na pasta da conta quando ela tem algum.
- "Retomar" no aviso de gravação com a sincronização pausada retoma o app inteiro.
- Sair pela bandeja com algo pendente pede confirmação na janela principal.
- Redefinir dados locais diz qual conta será redefinida; "Terminar de sincronizar" só diz que
  desconectou depois do último envio.
- "Mover tudo para…" do Lost and Found segue nas falhas e diz quantos itens foram, e recusa
  também uma montagem atrás de um link e as pastas do app.
- O aviso de pasta de outra pessoa só aparece quando alguém passa a ver o item.
- O menu "Disponível off-line" diz quando só parte dos itens está off-line.
- D-Bus: `SetMountPoint`; `ContentCachePath` deixa de pedir reinício; todo `LoginFinished` com
  erro traz um motivo estável; o atributo `user.gdl.state` ganha o valor `error`.
- O MCP recusa levar para o Drive uma pasta com links ou arquivos especiais.
- Clicar no corpo de um aviso abre a janela (antes dava erro no log).

## [0.11.0] - 2026-10-28

### Adicionado
- Bandeja: Gerenciar drives compartilhados, Ativar/Desativar o registro detalhado, Redefinir as
  preferências e Desconectar todas as contas (estes dois pedem confirmação na janela principal).
  Mais opções ganha Gerenciar drives compartilhados.
- Janela principal: cartão e visão "Drives compartilhados", com abrir na pasta ou na web e
  "Gerenciar drives compartilhados".
- Atividade: "Compartilhar" no menu de um item enviado; com os envios pausados, a fila aparece
  como "Pausado".
- Preferências com as abas "Meu computador" e "Google Drive" e as configurações atrás da
  engrenagem, como no original.
- Primeiro uso: passo de Resumo (conta, modo do Meu Drive, pasta e pastas escolhidas) e um tour da
  janela principal, também nas Preferências.
- Aviso do Lost and Found com "Mover tudo para…": leva as cópias guardadas para uma pasta
  escolhida, sem sobrescrever nada.
- Mensagens próprias para os limites do Drive: limite de download, pasta com níveis demais e Drive
  compartilhado cheio.
- Lista de erros: avisa quando os erros são de outra conta e troca para ela.
- Login: mensagens para bloqueio pela organização e conta desativada; depois de um minuto, ajuda
  para quando o login demora, com um link novo.
- Avisos de Drive compartilhado: mover para a pasta de outra pessoa ou para um Drive compartilhado
  (outras pessoas passam a ver) e Drive compartilhado com muita atividade (uma vez por dia).

### Mudado
- D-Bus: `MoveLostAndFound`; `AccountSummary` ganha a contagem de erros; categorias de erro
  `download-limit`, `folder-too-deep` e `shared-drive-full`; `LoginFinished` com os motivos
  `blocked-by-organization` e `account-disabled`.
- O banco ganha a coluna `owned_by_me` (migração automática).
- Ao entrar de novo numa conta, a página de login do Google já mostra essa conta (`login_hint`).

## [0.10.0] - 2026-10-27

### Adicionado
- Registro detalhado: liga e desliga em Mais opções sem reiniciar e volta ao normal depois de um
  dia. O serviço grava o log também em arquivo (`~/.local/state/gdrive-desktop-linux/logs`, 5
  arquivos de 10 MB).
- Gerar informações de diagnóstico (Mais opções e bandeja): um `.tar.gz` só do usuário na Área de
  trabalho com logs, relatórios de falha, sistema, preferências e contas, sem tokens, senhas nem
  e-mails inteiros.
- Enviar feedback (Mais opções e bandeja): Problema ou Sugestão, com a opção de juntar o pacote de
  diagnóstico; abre uma issue do projeto já preenchida no navegador.
- Relatórios de falha: cada programa guarda um relatório ao fechar de forma inesperada; com
  `AutomaticErrorReporting` ligado, o serviço oferece uma vez "Enviar relatório", que abre a issue
  já preenchida.
- Redefinir os dados locais (Mais opções): apaga o banco e o cache da conta e a lê de novo do
  Drive, com o mesmo login; recusa com envios pendentes.
- Avisos novos: sem permissão para criar, mover, renomear ou excluir; conta bloqueada pela
  organização; armazenamento quase cheio (90 % e 95 %); envios parados há mais de 3 dias; disco do
  computador cheio; itens das pastas do computador movidos para a lixeira; itens recuperados; e
  nova versão disponível (consulta diária às releases do GitHub, com "Ver novidades").
- Proteção contra ransomware: vinte arquivos regravados com cara de cifrados em um minuto pausam os
  envios da conta e avisam "Atividade suspeita nos arquivos", com "Verificar", "Eu reconheço esta
  atividade" e "Permitir sincronização". Formatos já comprimidos (fotos, vídeos, `.zip`…) não
  contam.
- Recuperação do banco: se os dados locais de uma conta estiverem danificados, o banco é guardado
  em `recuperacao/<data>`, as edições que não tinham subido vão para o Lost and Found
  ("Recuperados <data>") e a conta se lê de novo do Drive. Nada é reenviado sozinho.
- Servidor MCP `gdrive-desktop-linux-mcp` (stdio): estado de sincronização de um caminho, link da
  web, pastas acompanhadas, abrir a janela Compartilhar com um e-mail já digitado e adicionar uma
  pasta à sincronização em `backup` ou `stream` (a pasta vai para o Meu Drive e um link fica no
  lugar; a original vai para a lixeira do sistema).
- Políticas `TrustedRootCertsFile`, `TrustSystemRootCerts`, `DisableSSLValidation` (só no
  `policies.toml`) e `DisableLocalizedVirtualFolders` (nomes das pastas da montagem em inglês).
  `DisableCRLCheck` é aceita e não muda nada: o TLS do app não consulta listas de revogação.

### Mudado
- D-Bus: `VerboseLogging`, `SetVerboseLogging`, `GenerateDiagnostics`, `ResetLocalData`,
  `PathStatus`, `WebLinkOfPath`, `OpenShare` e `MoveToDrive`.
- A consulta de versão é o único acesso do app fora do Google (a política de privacidade diz o
  que vai nele).

### Segurança
- O pacote de diagnóstico, os relatórios de falha e o feedback passam por um filtro que tira
  tokens (`ya29.`, `1//`, `Bearer`, `access_token`…), `client_secret`, códigos de login, usuário e
  senha de endereços de proxy e mascara e-mails. Os relatórios ficam em pasta `0700`.

### Corrigido
- Uma pasta nova com vários arquivos, criada na pasta do Google Drive, subia os arquivos um de cada
  vez quando a criação da pasta demorava (rede lenta); agora sobem quatro juntos, como os demais.
- Avisos emitidos na partida da conta (login recusado) podiam se perder antes de o serviço começar
  a ouvir.
- Com o banco da conta estragado, o conteúdo das edições que não subiram não é mais apagado
  quando nem todo o banco dá para ler: o cache fica guardado ao lado.
- A pausa por atividade suspeita continua depois de reiniciar, e "Eu reconheço esta atividade" não
  desfaz mais uma pausa feita por você.
- "Redefinir os dados locais" sem conexão é recusado antes de apagar qualquer coisa.
- Pelo MCP, duas mudanças da mesma pasta ao mesmo tempo não desfazem mais a cópia uma da outra; as
  ferramentas dizem aos clientes quais só leem e qual move pastas.

## [0.9.0] - 2026-10-27

### Adicionado
- Modo Espelho: em Preferências › Google Drive, o Meu Drive inteiro vai para uma pasta comum do
  computador (sugerida ao lado da pasta do Google Drive, ou escolhida), com o tamanho a baixar e o
  espaço livre antes de confirmar. Documentos Google viram arquivos que abrem no navegador, e
  atalhos viram links. Voltar ao Stream espera os envios pendentes e pode mandar a pasta antiga
  para a lixeira. Os Drives compartilhados continuam na pasta do Google Drive.
- Pastas do computador: na aba "Meu computador", adicionar uma pasta a sincroniza nos dois
  sentidos com "Computadores/<este computador>" no Drive (configurador com conta, tamanho,
  contagem e opção de juntar com a pasta que já esteja lá). A pasta "Computadores" aparece na
  pasta do Google Drive, inclusive com os backups de outros computadores.
- Visão e cartão "Pastas conectadas" na janela principal, passo de escolha de pastas (com as
  pastas do usuário sugeridas) no primeiro uso e avisos de pasta que sumiu, foi movida, perdeu a
  permissão ou teria muitos itens excluídos de uma vez (a pasta para e pergunta).
- Nada se apaga de vez: excluir no computador manda para a lixeira do Drive, e o que sai do Drive
  vai para a lixeira do sistema. Edições nos dois lados ficam nas duas versões, com
  " (conflito <data>)" no nome.
- Dolphin: "Sincronizar esta pasta com o Google Drive" no botão direito de pastas fora do Drive e
  emblemas de estado nas pastas sincronizadas.
- Pendrives, cartões e câmeras: ao conectar, o app pergunta se faz backup (só envio, para
  "Computadores/<este computador>/<nome>") e avisa quando dá para remover com segurança. Na aba
  "Meu computador": fazer backup sempre, ignorar o dispositivo e desligar a pergunta.
- Políticas `DisableMirroredMyDrive`, `DisableMirroredFolders`, `DisableExternalMediaSync` e
  `PromptToBackupDevices`.

### Mudado
- D-Bus: `LocalFolders`, `CheckLocalFolder`, `AddLocalFolder`, `RemoveLocalFolder`,
  `RelocateLocalFolder`, `ConfirmFolderDeletes`, `Mirror`, `MirrorCheck`, `SetMirror`,
  `SetStream`, `Devices`, `BackupDevice`, `IgnoreDevice`, `ForgetDevice`, `FolderStates`,
  `FolderRoots` e o sinal `LocalFoldersChanged`.

### Segurança
- O corpo dos avisos vai escapado quando o servidor de notificações interpreta marcação: o nome de
  um arquivo ou o rótulo de um pendrive não viram link nem formatação.

### Corrigido
- Sair pela bandeja e abrir o app de novo pelo menu não leva mais à tela de primeiro uso quando
  há conta conectada: enquanto o serviço reconecta as contas, a bandeja e a janela mostram
  "Conectando à sua conta…".

## [0.8.0] - 2026-10-27

### Adicionado
- Várias contas: até 4 ao mesmo tempo, cada uma com a própria pasta (a primeira continua em
  `~/Google Drive`, as outras em `~/Google Drive (<e-mail>)`), o próprio cache e a própria fila.
  O menu da conta lista as contas, troca a conta mostrada, adiciona outra (reaproveitando o
  Client ID da primeira) e mostra "Entrar de novo" na conta cujo acesso expirou. As outras seguem
  sincronizando enquanto isso.
- "Desconectar todas as contas" em "Mais opções", com o mesmo fluxo de desconexão.
- Política `DisableAddAccount`; recusa com mensagem própria para conta já conectada, máximo de
  contas atingido e login feito com outra conta no "entrar de novo".
- Drives compartilhados na pasta "Drives compartilhados" de cada conta, com ícones de status,
  menu, miniaturas, pesquisa e Compartilhar. Drives com o mesmo nome ganham pastas distintas.
- Janela "Gerenciar drives compartilhados" (Preferências e menu do botão direito): ligar e
  desligar cada drive, com Salvar e Cancelar, e criar um Drive compartilhado novo.
- Mover um arquivo para um Drive compartilhado avisa que quem vê o arquivo muda; mover uma pasta
  do Meu Drive para um drive é recusado com um aviso que orienta a fazer isso pela web.
- "Adicionar atalho ao Drive" no menu do botão direito e em "Compartilhados comigo", escolhendo a
  pasta: o Meu Drive ou um Drive compartilhado que aceita itens novos, descendo pelas subpastas.
- A bandeja diz com quantas contas está conectada; "Pausar" e "Retomar" valem para todas.

### Mudado
- O D-Bus recebe a conta nos métodos que não trazem caminho, e `Status` lista as contas.
  A linha de comando ganha `--account <e-mail>`.

### Corrigido
- Um arquivo aberto podia dar erro de leitura logo depois que o envio terminava e o id
  provisório virava o id do Drive.
- O número de envios pendentes contava as tarefas da fila, não os arquivos (um arquivo editado
  duas vezes contava dois).

## [0.7.0] - 2026-10-27

### Adicionado
- Ícones de status no Dolphin: na nuvem, disponível off-line, sincronizando e em conflito (plugin
  KF6 de emblemas, nos pacotes `.deb`, `.rpm` e Arch). O serviço avisa pelo D-Bus
  (`StatesChanged`) quais arquivos mudaram, e o Dolphin redesenha só esses.
- Janela "Compartilhar", aberta pelo menu do botão direito: adicionar pessoas com papel
  (leitor, comentarista, editor), com ou sem aviso por e-mail e mensagem; mudar o papel ou tirar
  o acesso de quem já tem; acesso geral (restrito, domínio da conta Workspace ou qualquer pessoa
  com o link) e copiar o link.
- Janela "Gerenciar versões", aberta pelo menu: as versões guardadas do arquivo, da atual para a
  mais antiga, com baixar (na pasta de downloads, sem sobrescrever nada), restaurar (vira uma
  versão nova; recusa se houver edição local ainda não enviada) e manter para sempre.
- Menu do botão direito: arquivos do Word, Excel e PowerPoint ganham "Abrir no Documentos /
  Planilhas / Apresentações Google" (o editor on-line), respeitando a política
  `OpenOfficeFilesInDocs`.
- Copiar link avisa quem pode abrir: só quem tem acesso, pessoas do domínio ou qualquer pessoa
  com o link, e com que papel.
- A pasta do Drive entra nas pastas que o indexador do KDE (Baloo) não lê, preservando as outras
  exclusões; sem isso ele baixaria o Drive inteiro. Desinstalar desfaz.

### Corrigido
- Cancelar um envio logo depois de reabrir o arquivo para escrita podia tirar o conteúdo de baixo
  do programa que grava: o fechamento atrasado da abertura anterior apagava a marca de "aberto".
- Com a máquina ocupada, a leitura sequencial fazia um pedido a mais ao Drive (a leitura seguinte
  chegava antes da antecipada e pedia o bloco à parte).

## [0.6.0] - 2026-10-26

### Mudado
- Desempenho: o serviço parado gastava ~2,5% de um núcleo com um Drive grande (~610 mil itens),
  porque a busca de arquivos off-line a baixar, a cada 30 s, percorria todos os itens. Essa busca,
  a contagem do estado da bandeja, a lista de pendentes da Atividade, a tela de arquivos off-line
  e o menu do botão direito agora usam só os índices dos itens fixados ou editados (índice novo
  para os editados).
- Desempenho: abrir pela primeira vez uma pasta sem imagens, vídeos nem PDFs (código, textos,
  compactados) não espera mais as miniaturas nem consulta o Google. Buscas, cópias, backups,
  controle de versão e IDEs (`find`, `rsync`, `git`, `code` etc.) percorrem as pastas sem semear
  miniaturas, como já faziam os indexadores; a pasta de fotos continua com as miniaturas na
  primeira abertura pelo gerenciador de arquivos.
- Desempenho: o download sequencial (cópia, player de vídeo) ia em pedidos de 8 MiB, um de cada
  vez e só usáveis depois de chegarem inteiros (~1,2 MB/s). Agora cada bloco de 1 MiB fica
  disponível assim que chega, a leitura antecipada cresce de 8 até 64 MiB enquanto a leitura
  segue sequencial (um arquivo de 64 MiB passou de 16 para até 7 pedidos) e a próxima janela é
  pedida enquanto a anterior ainda chega. A leitura antecipada só começa depois de 1 MiB lido em
  sequência, então saltos (o player procurando a cena) baixam só os blocos lidos: 8 saltos de
  256 KiB num vídeo baixam 16 MiB (eram 32 MiB). Cada download usa ~1 MiB de memória em vez de
  8 MiB.
- Desempenho: os envios iam um de cada vez (~2 s por arquivo pequeno; 50 arquivos de 20 KB em
  105 s). Agora até quatro sobem juntos, cada um com um arquivo diferente; versões do mesmo
  arquivo continuam uma de cada vez e em ordem, e a pasta nova sobe antes do que está nela
  (quem espera a pasta olha de novo a cada 1 s, eram 5 s). Com a fila parada, o serviço segue
  com uma consulta por segundo.

### Corrigido
- `gdrive-desktop-linuxd --version` (ou o serviço iniciado de novo) subia uma segunda instância
  que tomava o nome no D-Bus, montava "~/Google Drive (1)" e reiniciava a primeira. Agora
  `--version` e `--help` só respondem, e uma segunda instância percebe a primeira e sai sem
  tocar em nada.
- Compilado sem todas as dependências já baixadas (CI com cache novo), o Sobre saía sem a lista
  de softwares de terceiros: a lista agora baixa o que falta, sempre com o `Cargo.lock` como está.

## [0.5.0] - 2026-10-26

### Adicionado
- Menu "Google Drive" no botão direito, só nos itens do Meu Drive: abrir no navegador, copiar link,
  Disponível off-line / Somente on-line (com a marca atual), novo Documento, Planilha ou
  Apresentação Google na pasta, atualizar a pasta e ver a lixeira. O serviço decide os itens
  (métodos D-Bus `Menu` e `MenuAction`) e cada gerenciador só desenha:
  - Dolphin: plugin KF6 nos pacotes `.deb`, `.rpm` e Arch;
  - Nautilus, Nemo, Caja e Thunar: uma extensão Python em todos os formatos (os pacotes sugerem
    os bindings de cada um);
  - COSMIC Files: ações soltas "Google Drive: …", gravadas pelo serviço na configuração dele e
    removidas na desinstalação.
- `gdrive-desktop-linux menu-action <ação> <caminho>…`, com o link na área de transferência e os
  erros em notificação.

### Mudado
- README atualizado, com todas as telas; as imagens saem de um teste de ponta a ponta
  (`GDL_SCREENSHOTS`) com o Drive simulado.
- O modo espelho nas preferências diz "Chega numa próxima versão" (dizia "na versão 0.4").

## [0.4.0] - 2026-10-26

### Adicionado
- Desinstalador em todos os formatos: o item "Desinstalar Google Drive" no menu de aplicativos e o
  botão "Desinstalar…" no Sobre abrem uma janela que pergunta se apaga também os dados deste
  computador (com envios pendentes, as mesmas escolhas de "Desconectar conta"); depois desmonta a
  pasta, tira o início automático e o lugar nos Locais do Dolphin e remove os programas (pacote:
  com a senha de administrador; `.tar.gz`: só o que a instalação registrou). As cópias do Lost and
  Found nunca são apagadas.
- `gdrive-desktop-linux uninstall [--purge] [--yes]` e o `uninstall.sh` no `.tar.gz`.

### Corrigido
- Desinstalado pelo gerenciador de pacotes, o app deixava no início da sessão uma entrada para um
  programa que não existia mais: agora ela é ignorada (`TryExec`).

## [0.3.2] - 2026-10-26

### Adicionado
- Instaladores em cada release do GitHub: `.deb` (Ubuntu 22.04+, Debian 12+, Mint), `.rpm`
  (Fedora, openSUSE), `.pkg.tar.zst` (Arch, Manjaro) e um `.tar.gz` para qualquer distribuição,
  com `SHA256SUMS`. Uma tag de versão gera, testa a instalação de cada um na sua distribuição e
  publica o release.
- `scripts/package.sh` gera os mesmos instaladores localmente; `install-dev.sh --destdir`.

### Corrigido
- Logo depois de instalar, a janela dizia que o serviço não respondia até sair e entrar de novo na
  sessão (o barramento só lê os serviços novos ao iniciar): a bandeja, a janela e a CLI pedem que
  ele releia a configuração quando não conhecem o serviço.

## [0.3.1] - 2026-10-26

### Corrigido
- O botão da conta mostrava só a inicial do e-mail; a foto aparecia apenas ao abrir o menu. Agora a
  foto do perfil aparece no botão assim que a janela abre.
- Com a janela em tela cheia, o menu da conta saía pela borda direita: a posição era calculada antes
  de chegarem a foto, o nome e o uso. Agora o menu se reposiciona quando o conteúdo muda de tamanho.

## [0.3.0] - 2026-10-26

### Adicionado
- Miniaturas das imagens vindas do Google, sem baixar os arquivos: o serviço grava as miniaturas
  no cache do freedesktop (Dolphin e outros gerenciadores que o usam), então abrir uma pasta de
  fotos não enche o cache de conteúdo.
- A primeira abertura de cada pasta espera até 1,5 s pelas miniaturas; o resto segue em segundo
  plano e as aberturas seguintes não esperam.
- As miniaturas gravadas são apagadas ao sair da conta.

## [0.2.5] - 2026-10-26

### Corrigido
- A janela aparecia sem o ícone do app na barra de tarefas e na barra de título (KDE Wayland): ela
  se anunciava com o nome do programa, que não corresponde a nenhum `.desktop`. Agora usa o id do app.
- Os testes de ponta a ponta abriam as janelas na sessão gráfica de quem os rodava, e não no
  servidor gráfico de teste: o GTK caía no Wayland padrão. Agora ficam só no Xvfb.

## [0.2.4] - 2026-10-26

### Adicionado
- Site do projeto no GitHub Pages (página inicial, Política de Privacidade e termos), exigido pelo
  Google para publicar a tela de consentimento.

### Mudado
- Guia do Client ID mais detalhado: a parte da tela de consentimento traz, com botões de copiar, a
  página inicial, a política, os termos e o domínio autorizado, e avisa para não enviar logotipo; os
  escopos lembram o Salvar no fim da página; a publicação explica o aviso de verificação e o modo de
  teste. Novo problema comum: "Para publicar o app, conclua a configuração na página de branding".

## [0.2.3] - 2026-10-26

### Corrigido
- Depois da instalação, a janela dizia que o serviço não estava respondendo até a próxima sessão:
  o barramento da sessão não conhecia a ativação recém-instalada. `install-dev.sh` agora pede ao
  D-Bus que releia a configuração.

## [0.2.2] - 2026-10-26

### Mudado
- Ícone novo do app (menu de aplicativos, janela, tela Sobre e README), em PNG nos tamanhos do
  tema; `scripts/update-app-icon.py` gera todos a partir de `assets/icons/app-icon-source.png`.
  A instalação remove o ícone SVG antigo.

### Corrigido
- O selo de versão do README tinha ficado em 0.2.0.

## [0.2.1] - 2026-10-26

### Adicionado
- Primeiro uso com o guia do Client ID dentro do app: as seis partes no Google Cloud (projeto,
  Drive API, tela de consentimento, escopos, publicação e credencial), cada uma com a explicação e
  um botão que abre a página certa, botões para copiar os escopos e os problemas mais comuns com o
  que fazer. A espera do login explica o aviso "O Google não verificou este app".

## [0.2.0] - 2026-10-26

### Adicionado
- Plano de implementação da versão 0.2 (interface) em `docs/superpowers/plans/` e o registro das
  decisões em `docs/superpowers/decisoes/`.
- Contrato D-Bus num crate próprio (`gdl-ipc`), usado pelo serviço, pela CLI, pela bandeja e pela
  janela, com os erros do serviço já tipados.
- Serviço:
  - estado detalhado da sincronização (sincronizado, sincronizando N itens, pausado, sem internet,
    erro, armazenamento cheio) e dados da conta (nome, e-mail, foto e uso do armazenamento);
  - atividade da sincronização com o progresso de cada envio e download;
  - cancelar um envio sem perder a versão local, que vai para `Uploads cancelados` com data e hora
    no nome;
  - lista de erros por categoria (cota, limite de envio, sem permissão, grande demais, apagado na
    nuvem ou aqui, Lost and Found), com "Tentar de novo";
  - histórico de notificações, ações nos avisos e "não mostrar de novo" para os avisos opcionais;
  - resumo do uso do disco e itens off-line por pasta, "Limpar cache", "Limpar arquivos
    off-line", pesquisa por nome, itens recentes e "Compartilhados comigo";
  - preferências pelo serviço, com as políticas do administrador bloqueando as chaves
    gerenciadas, aviso quando a mudança pede reinício e reinício no mesmo processo;
  - início automático na sessão e a montagem nos Locais do Dolphin, conforme as preferências;
  - limite de banda de envio e de download, proxy (sistema, direto ou um endereço HTTP/SOCKS
    validado ao gravar, sem a URL no log) e pausa em conexão limitada.
- Bandeja (`gdrive-desktop-linux-tray`): ícone que muda com o estado (animado ao sincronizar),
  dica com o resumo e menu com fazer login, abrir a pasta, abrir o Drive no navegador, pesquisa,
  pausar/retomar, erros, arquivos off-line, preferências, Sobre e sair (com o serviço fora do ar,
  "Iniciar o serviço"). Atalho global da pesquisa pelo portal do KDE
  (sem portal, segue sem atalho) e uma bandeja só por sessão: abrir outra mostra a janela
  principal.
- Janela (`gdrive-desktop-linux-ui`, Tauri), em pt-BR e inglês, com tema do sistema ou claro e uma
  instância só (abrir de novo traz a tela pedida):
  - janela principal: estado da sincronização com pausar, Início com os cartões (precisa de
    atenção, off-line, compartilhados, status), atividade com progresso e cancelar, compartilhados
    comigo, notificações e links rápidos, menu da conta e sair (perguntando se ainda há envios);
  - primeiro uso: Client ID e segredo, login no navegador (com "Gerar link" para copiar),
    escolha do que fazer com a pasta e o passo de off-line;
  - preferências em três abas (Meu computador, Google Drive e Configurações), preferências de
    notificação e Sobre com a lista de licenças de terceiros;
  - pesquisa rápida flutuante: recentes com o campo vazio, resultados com ícone e pasta, setas,
    Enter abre e Esc ou a perda de foco fecham;
  - lista de erros agrupada por categoria, com explicação, 10 por página, "Abrir pasta" e
    "Tentar de novo";
  - arquivos off-line: barra de uso do disco (off-line, cache, livre), itens pasta a pasta,
    "Remover acesso off-line" e as ações de cache.
- Instalação para uso sem root (`scripts/install-dev.sh`, em `~/.local` ou noutro prefixo):
  programas, serviço do usuário, ativação do serviço e da janela pelo D-Bus, atalho no menu,
  ponteiros `.gdoc` e ícones. A janela aceita `--dbus` (ativação sem abrir tela).
- Testes da interface: `node --test` para a lógica das telas e `scripts/e2e.sh` (Xvfb, barramento
  próprio, openbox e tauri-driver) para a janela e a bandeja de verdade sobre o Drive simulado; o
  CI ganhou as bibliotecas da janela, os testes JS e um job de ponta a ponta.

### Mudado
- Os atalhos `.desktop` e o início automático apontam para o caminho completo dos programas
  (entre aspas), para funcionarem também fora do `PATH`.
- O build de release falha se não conseguir montar a lista de licenças de terceiros.

### Corrigido
- Itens compartilhados por outras pessoas apareciam na pesquisa e nos recentes com um caminho que
  não existe na montagem.
- Ler um arquivo novo no instante em que o envio terminava podia dar erro de E/S.
- Um `preferences.toml` editado à mão e ilegível impedia o serviço de subir; agora vale como
  vazio, com aviso no log.
- A janela e a bandeja não percebiam a queda do serviço (e o aviso "serviço indisponível" podia
  ficar preso depois da volta).
- `ContentCachePath` e `DefaultMountPoint` aceitavam caminhos relativos ou uma pasta sem
  permissão de gravação, e a conta não subia na partida seguinte.
- Cancelar o envio de um arquivo ainda aberto para escrita tirava o conteúdo do programa que
  gravava; agora o cancelamento é recusado até o arquivo ser fechado.
- Enter logo depois de digitar na pesquisa abria um resultado da lista anterior.
- Sem rede, a pesquisa rápida podia esperar até um minuto; agora espera a API por 2,5 s e mostra
  os nomes do banco.

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

[Não lançado]: https://github.com/junior9283/gdrive-desktop-linux/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.11.0...v1.0.0
[0.11.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.2.5...v0.3.0
[0.2.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/junior9283/gdrive-desktop-linux/releases/tag/v0.0.1
