# Design — gdrive-desktop-linux

- **Data:** 25/10/2026
- **Situação:** aprovado pelo dono do projeto em 25/10/2026
- **Referência de paridade:** Google Drive para computador, Windows, versão 133.0 (19/10/2026)

## 1. Intenção

O dono do projeto usa Linux (Nobara/Fedora com KDE Plasma 6) e quer **o Google Drive para computador
do Windows, só que nativo de Linux**: o mesmo comportamento, a mesma aparência e cada funcionalidade
e detalhe da versão Windows. A experiência esperada é instalar, entrar na conta pelo navegador e ter
uma pasta em que os arquivos ficam na nuvem até serem abertos, com ícone na bandeja, "Disponível
off-line", backup de pastas, várias contas, Drives compartilhados e integração com o gerenciador de
arquivos.

Hoje ele usa montagens genéricas (rclone via systemd), que funcionam mas não oferecem essa
experiência. **Nada do motor do rclone deve ser usado nem servir de base:** o motor é próprio.

O projeto é público (GitHub, GPL-3.0) para receber contribuições.

## 2. Objetivos

1. Paridade de funcionalidades com a versão Windows, conforme o catálogo da seção 6, exceto o que a
   seção 3 exclui.
2. Paridade de comportamento: os mesmos fluxos, estados, confirmações e mensagens, com textos
   próprios em pt-BR e inglês.
3. Paridade de aparência: o mesmo layout e a mesma organização de janelas e menus, com o visual
   Material do original, mas com fonte e ícones livres e sem nenhum arquivo do Google.
4. Integração nativa com o Linux: FUSE, bandeja StatusNotifierItem, notificações freedesktop, Dolphin
   (KDE) primeiro, systemd e Secret Service.
5. Confiabilidade igual ou melhor que a do original: nenhum dado do usuário pode se perder.

**Critério de paridade (definido com o dono do projeto):** paridade significa o **comportamento
visível ao usuário** (o que ele vê, aciona e recebe), não os mesmos mecanismos internos. Onde a API
pública não permitir o mesmo resultado, o app chega o mais perto possível, e a diferença entra na
lista "Diferenças conhecidas" do README e na matriz de paridade (seção 9) como risco aceito.

## 3. Fora do escopo

| Item do original | Motivo |
|---|---|
| Backup para o Google Fotos | O próprio Google descontinuou (sem novas pastas desde 15/06/2026, suporte encerrado em 10/08/2026). |
| Presença em tempo real no Microsoft Office | Depende do Microsoft Office, que não existe no Linux. |
| Complemento do Outlook e plugin do Meet | Dependem do Outlook para Windows. |
| Criptografia do lado do cliente (CSE) | Depende da infraestrutura de chaves corporativas do Workspace. Pode voltar numa versão futura. |
| Painel interno de depuração (`monitor_dialog`) e pesquisas de satisfação | São ferramentas internas do Google. |
| Integração com Spotlight e Apple Photos | Exclusivas do macOS. |
| Migração do antigo "Backup and Sync" | Não existe no Linux. |
| Avisos de versão obsoleta controlados pelo servidor do Google | Substituídos pelo aviso de nova versão do próprio projeto (seção 6.14). |

## 4. Anti-objetivos

O projeto fracassa, mesmo que funcione tecnicamente, se:

- **perder dados:** um arquivo local não enviado ser apagado, ou um arquivo da nuvem ser apagado
  permanentemente por ação do app;
- **o "sob demanda" virar baixar tudo:** o gerenciador de arquivos, as miniaturas ou o indexador
  (Baloo) lerem o conteúdo dos arquivos só por navegar nas pastas;
- **contiver material do Google:** código, textos copiados, ícones, logotipos, a fonte Google Sans ou
  binários;
- **se passar pelo app oficial:** usar o Client ID OAuth do Google, as APIs internas
  (`drivefrontend-pa`, `drive/v2internal`) ou os canais de notificação internos;
- **coletar dados sem consentimento:** telemetria ou relatório de erros ligados por padrão;
- **parecer um app web lento:** janelas que demoram a abrir ou a pasta travando o Dolphin.

## 5. Restrições

### 5.1 Legais e de processo

- **Clean room:** o app original foi estudado (telas, textos e strings dos binários, ajuda oficial e
  Ghidra) só para entender o comportamento. As anotações de engenharia reversa ficam em `re/`, que é
  ignorada pelo git e nunca é publicada. Este documento e o código descrevem o comportamento com
  palavras e implementação próprias.
- **Marca:** o nome não usa "Google". A interface diz "Google Drive" apenas para nomear o serviço e
  sempre como "cliente não oficial".

### 5.2 API do Google (só a pública)

- O app usa a **Drive API v3** (files, changes, drives, permissions, revisions, about), o endpoint de
  miniaturas (`thumbnailLink`) e OAuth 2.0 para app de desktop: loopback com PKCE, igual ao fluxo do
  original.
- **Escopo `drive` (restrito):** para distribuir a um público amplo, o app OAuth do projeto precisa
  passar pela verificação do Google e por uma avaliação de segurança anual (CASA). Até lá, aparece a
  tela de "app não verificado", com limite de 100 usuários.
- **Decisão do dono do projeto:** nas versões 0.x, **cada usuário usa o próprio Client ID** (tipo
  "App para computador" criado no Google Cloud dele). O primeiro uso pede o Client ID e o segredo
  (um tipo de app em que o segredo não é confidencial), guarda os dois no Secret Service, e o projeto
  publica um guia passo a passo em `docs/`. O guia orienta a publicar o app OAuth "em produção" sem
  verificação, porque o modo "teste" faz o login expirar a cada 7 dias. A decisão de criar um app
  OAuth do projeto e pagar a verificação/CASA fica para antes da 1.0. Nenhum Client ID ou segredo
  entra no repositório.
- **Mudanças vindas da nuvem:** não existe push para apps de desktop. O app consulta
  `changes.list` periodicamente (padrão de 15 s, ajustável; o original consulta a cada 10 s).
- **"Computadores":** a API pública não cria a seção "Computadores" que o original usa para backups.
  **Decisão (minha):** os backups vão para `Meu Drive/Computadores/<nome-do-computador>/`, e a
  montagem mostra uma pasta virtual `Computadores` no primeiro nível que aponta para lá, para manter a
  aparência do original. Isso é definitivo para o plano; se um teste com conta real mostrar que a API
  permite a seção verdadeira, a mudança fica restrita ao destino do backup.
- **Limites da API:** respeitar a cota por usuário, com recuo exponencial nos erros 403/429 da
  `userRateLimitExceeded`.

### 5.3 Plataforma

- **Plataforma principal:** Fedora/Nobara 44+ com KDE Plasma 6, Wayland e X11. Outras distribuições e
  outros ambientes de desktop funcionam sem a integração do Dolphin.
- **Requisitos:** FUSE 3 (`/dev/fuse`), Secret Service (KWallet ou GNOME Keyring), sessão D-Bus e
  WebKitGTK (para o Tauri).

## 6. Catálogo de funcionalidades (o que o app faz)

Cada item descreve o comportamento esperado. Detalhes técnicos de implementação ficam a critério de
quem constrói, salvo quando indicados.

### 6.1 Contas e login

- **Login pelo navegador** padrão (`xdg-open`), com servidor HTTP local em loopback e PKCE.
  - Tempo limite de 5 minutos.
  - O botão "Gerar link" copia a URL de login para colar em outro navegador.
  - Página de resultado própria (sucesso ou falha) servida pelo app.
- **Até 4 contas** simultâneas.
  - Adicionar, alternar a conta exibida, reautenticar e desconectar uma conta ou todas.
  - Erros com mensagem própria: conta já conectada, máximo atingido, autenticação expirada, conta
    bloqueada ou desativada pela organização, sem internet e login demorando.
- **Tokens:** o refresh token fica no Secret Service, nunca em arquivo de texto.
- **Fluxo de desconexão:**
  - mostra quantos arquivos ainda não sincronizaram e oferece três escolhas: terminar de sincronizar
    e sair, mover os não sincronizados para uma pasta escolhida, ou excluí-los, com confirmação
    explícita;
  - avisa que os arquivos off-line da conta serão removidos;
  - tem telas de progresso.
- **Política `AllowedAccountsPattern`** (regex) e **`DisableAddAccount`**.

### 6.2 Pasta do Drive: modos Stream e Espelho

- **Stream (padrão):** uma montagem FUSE por conta.
  - Local padrão `~/Google Drive`, e para as demais contas `~/Google Drive (<e-mail>)`. O local é
    configurável por conta (preferência `DefaultMountPoint`); se estiver ocupado, o app avisa e usa
    um local alternativo, como o original.
  - Conteúdo da montagem: `Meu Drive`, `Drives compartilhados` (só se houver),
    `Computadores` (só se houver) e as pastas ocultas de apoio (`.shortcut-targets-by-id`, a pasta
    de versões, a lixeira e `Lost and Found`). Os nomes seguem o idioma do sistema; a política
    `DisableLocalizedVirtualFolders` força os nomes em inglês.
  - A montagem também aparece em "Locais" do Dolphin (preferência `ShowMountPointInSidebar`).
- **Espelho:** o Meu Drive vira uma pasta local real com sincronização nos dois sentidos. A pasta
  local é observada com inotify e reconciliada com `changes.list`.
  - Drives compartilhados são sempre em Stream, como no original.
  - **Troca entre modos** com as mesmas salvaguardas do original:
    - De Espelho para Stream: verifica uploads pendentes, mantém a pasta antiga e oferece removê-la
      depois.
    - De Stream para Espelho: a pasta destino precisa estar vazia; arquivos idênticos não são
      duplicados; se o conteúdo local e o da nuvem diferem, as duas versões são mantidas.
- **Sem o limite de 4 GB por arquivo** da unidade FAT do Windows. O limite é o da nuvem (5 TB).
- **Documentos Google** (Docs, Planilhas, Apresentações, Formulários, Desenhos, Mapas, Sites etc.)
  aparecem como arquivos ponteiro pequenos (`.gdoc`, `.gsheet`, `.gslides`…) em formato JSON próprio,
  com um tipo MIME registrado.
  - Abri-los leva ao navegador.
  - Copiar um ponteiro cria um atalho, e o app avisa disso.
- **Atalhos do Drive** aparecem como links simbólicos para `.shortcut-targets-by-id/<id>/…`.
- **Nomes:**
  - `/` num nome do Drive aparece como `∕` (U+2215) e volta como `/` ao enviar.
  - Nomes duplicados na mesma pasta recebem um sufixo estável, ` (1)`, ` (2)`…, ordenado por data de
    criação e nunca renomeado na nuvem.
- **Excluir na pasta** manda para a lixeira do Drive, nunca para exclusão permanente.

### 6.3 Arquivos sob demanda, cache e off-line

- **Metadados de todo o Drive** num banco SQLite por conta, para que as pastas abram na hora mesmo
  sem internet e após reiniciar.
- **Leitura sob demanda em blocos:** só os trechos lidos são baixados. Uma tabela de intervalos
  registra o que já está no disco, e o app faz leitura antecipada quando a leitura é sequencial.
- **"Disponível off-line"** (fixar) em arquivos e pastas.
  - Baixa tudo em segundo plano e mantém atualizado.
  - "Somente on-line" libera o espaço.
  - O estado misto aparece quando uma pasta tem parte do conteúdo fixada.
- **Gravações** vão primeiro para o cache local e entram numa fila de envio persistente, que
  sobrevive a reinícios e quedas de rede. Uploads grandes são retomáveis.
- **Cache:**
  - pasta configurável (`ContentCachePath`), com migração e tela de progresso;
  - limite de tamanho (`ContentCacheMaxKbytes`), que não conta arquivos fixados nem pendentes de
    envio;
  - espaço livre mínimo (`MinFreeDiskSpaceKBytes`, padrão de 500 MB): abaixo dele, o app para de
    gravar no cache e avisa;
  - remoção pelo critério de uso menos recente.
- **Tela "Arquivos off-line / armazenamento"** (equivalente à `file_stats_dialog`):
  - uso de disco por conta numa barra segmentada (off-line, cache);
  - navegação por pastas com breadcrumb;
  - remover acesso off-line por item;
  - "Limpar cache", "Limpar arquivos off-line" e atalho para as configurações de cache.
- **Sem internet:**
  - o que está no cache ou fixado continua acessível;
  - abrir o que não está baixado dá erro de E/S claro, com uma notificação "Sem conexão";
  - as edições ficam na fila.
- **Pausar e retomar a sincronização** (bandeja e janela principal).
  - Opção de **pausar em conexão limitada**, detectada pelo NetworkManager.
  - Ao salvar um arquivo com a sincronização pausada, o app pergunta se quer retomar.

### 6.4 Sincronização, conflitos e recuperação

- Envio e recebimento automáticos, com progresso por item (na fila, enviando %, concluído,
  cancelando, cancelado, baixando, pausado, movendo para a lixeira, restaurando).
- **Cancelar upload**, com o aviso "concluiu antes de cancelar" quando for o caso. Os uploads
  cancelados vão para uma pasta "uploads cancelados" que o usuário pode revisar.
- **Conflito.** Semântica decidida por mim, seguindo o original:
  - **Detecção:** cada item guarda no SQLite a revisão-base, que é a `headRevisionId`/`md5Checksum`
    da última sincronização. Há conflito quando a nuvem passou da revisão-base e existe edição local
    pendente sobre essa base.
  - **Stream:** a **versão da nuvem fica no caminho original**. A edição local é preservada em
    `Lost and Found`, uma pasta **local** por conta dentro do diretório de dados do app, que aparece
    na montagem como pasta virtual oculta. O app notifica com as ações "Ver arquivos" e "Mover tudo
    para…". Nada é enviado automaticamente.
  - **Envio impossível** (sem permissão de edição, pasta destino somente leitura ou inacessível): o
    mesmo tratamento, com a versão local em `Lost and Found` e notificação.
  - **Item excluído na nuvem com edição local pendente:** o app não recria o arquivo na nuvem. A
    versão local vai para `Lost and Found` e notifica. O usuário pode movê-la de volta para a pasta,
    o que gera um novo envio.
  - **Espelho:** usa a mesma revisão-base. Na divergência, as duas versões são mantidas lado a lado
    (a local recebe um sufixo de conflito com data) e as duas sobem.
  - O formato exato do sufixo fica a critério de quem constrói.
- **Recuperar arquivos não sincronizados** e recuperação automática após falha do banco local
  (reconstrução a partir da nuvem, preservando o que estava pendente).
- **Detecção de ransomware:** detectar por heurística a modificação ou renomeação em massa de
  arquivos com conteúdo de alta entropia num curto intervalo.
  - O app pausa os envios e mostra um aviso com as opções "Verificar", "Eu reconheço esta atividade"
    e "Permitir sincronização".
  - O aviso explica que as versões antigas ficam disponíveis no Drive.
- **Limites do Drive** viram mensagens claras:
  - cota cheia;
  - limite diário de upload (750 GB);
  - limite de download excedido;
  - 100 níveis de pasta e 500 mil itens em Drive compartilhado;
  - sem permissão.
- **Lista de erros** (equivalente à `sync_error_dialog`):
  - agrupada por categoria e paginada;
  - ações por item (abrir pasta, tentar de novo);
  - aviso de que outra conta tem erros, com botão para trocar de conta.

### 6.5 Backup de pastas do computador e dispositivos

- **Aba "Meu computador"** nas Preferências:
  - adicionar pasta;
  - configurador por pasta: conta, destino Drive, tamanho e contagem;
  - parar a sincronização, com confirmação;
  - mesclar com uma pasta existente.
  - As pastas sugeridas são as XDG: Área de trabalho, Documentos, Imagens e Vídeos.
- **Pastas conectadas:** sincronização nos dois sentidos com o destino definido na seção 5.2
  ("Computadores").
  - Avisos quando a pasta some ("Localizar", "Parar de sincronizar"), foi movida ou perdeu
    permissão.
  - Não é permitido aninhar pastas sincronizadas.
- **Dispositivos removíveis (USB, cartão, câmera)**, detectados pelo UDisks2:
  - notificação perguntando se quer fazer backup ("Lembrar minha escolha");
  - lista de dispositivos ignorados;
  - backup único de upload, que avisa quando é seguro remover o dispositivo.
  - Política `DisableExternalMediaSync`; preferência "Perguntar se quero fazer backup de
    dispositivos".
- **Compartilhamentos de rede (NAS):** backup periódico a cada 6 h, como o original.
- **Políticas** `DisableMirroredMyDrive` e `DisableMirroredFolders`.

### 6.6 Drives compartilhados, "Compartilhados comigo" e atalhos

- **Drives compartilhados:**
  - aparecem na montagem;
  - tela "Gerenciar drives compartilhados" para ligar ou desligar a sincronização de cada drive,
    com alterações pendentes e botões salvar/cancelar;
  - aviso de drive com alta atividade.
- **Criar Drive compartilhado:** pela tela própria e pelo menu de contexto da pasta "Drives
  compartilhados". O nome tem que ser único.
- **"Compartilhados comigo":**
  - lista na janela principal, com data de compartilhamento;
  - "Adicionar atalho" no local escolhido;
  - "Abrir na web".
- **Avisos ao mover** itens para dentro, para fora ou entre Drives compartilhados e pastas
  compartilhadas (mudança de permissões), com a opção "não mostrar de novo".
  - Mover uma pasta do Meu Drive para um Drive compartilhado não é suportado, como no original, e a
    mensagem orienta a usar a web.

### 6.7 Janela principal (equivalente à `home_dialog`)

Abre pelo clique no ícone da bandeja e tem o mesmo layout do original.

- **Cabeçalho:**
  - status da sincronização (concluída, incompleta, sincronizando N itens com o tempo restante, sem
    internet, armazenamento cheio);
  - "Última sincronização há…";
  - botão Pausar/Retomar;
  - campo "Pesquisar no Google Drive".
- **Menu da conta:**
  - contas com foto e uso de armazenamento;
  - adicionar conta, trocar de conta e desconectar;
  - "Gerenciar armazenamento" (web), Política de Privacidade e Termos (os do projeto).
- **Visões:**
  - **Início:** cartões "Precisa de atenção", "Arquivos disponíveis off-line",
    "Compartilhados comigo", "Drives compartilhados", "Pastas conectadas", "Status de
    sincronização" e "Gemini" (link para a web);
  - **Atividade de sincronização:** tabela paginada e ordenável. O menu por linha tem "Abrir no
    gerenciador de arquivos", "Abrir na web", "Copiar link", "Compartilhar" (só depois do upload) e
    "Cancelar upload";
  - **Pastas conectadas**;
  - **Arquivos off-line**;
  - **Compartilhados comigo**;
  - **Drives compartilhados**;
  - **Notificações**, com contador de não lidas;
  - **Links rápidos:** Drive na web, perguntas frequentes e ajuda off-line, apontando para a
    documentação do projeto.
- **Menu "Mais opções":**
  - Arquivos off-line;
  - Lista de erros (com contagem);
  - Gerenciar drives compartilhados;
  - Enviar feedback;
  - Ativar ou desativar o registro detalhado;
  - Gerar informações de diagnóstico (arquivo na Área de trabalho);
  - Redefinir as preferências;
  - Desconectar todas as contas;
  - Preferências;
  - Sobre;
  - Sair (com confirmação se houver itens sincronizando).
- **Atalho de teclado** que foca o painel de status.

### 6.8 Ícone na bandeja

- **StatusNotifierItem** (biblioteca `ksni`).
- **Estados do ícone:** sincronizado, sincronizando (animado), erro, pausado, sem internet e sem
  login.
- **Dica (tooltip):** "Tudo sincronizado", "Sincronizando N arquivos", "Sem internet (N para
  sincronizar)", "Pausado (N para sincronizar)", "Armazenamento cheio", "Você não fez login",
  "Conectado com N contas".
- **Cliques:** o esquerdo abre a janela principal; o direito abre o menu, com as mesmas ações do menu
  "Mais opções" mais "Abrir a pasta do Google Drive", "Abrir o Google Drive no navegador", Pesquisar e
  Pausar/Retomar.

### 6.9 Pesquisa rápida (equivalente à `search_dialog`)

- Janela flutuante aberta por **atalho global**, padrão Ctrl+Alt+G e configurável. O atalho é
  registrado pelo portal XDG GlobalShortcuts (Wayland) ou pelo KGlobalAccel.
- Pesquisa por nome e conteúdo na conta selecionada:
  - consulta a Drive API (`fullText contains`, que exige rede) e o banco local; sem internet, a
    pesquisa usa só os nomes do banco local;
  - resultados com ícone, nome e caminho;
  - mostra sugestões recentes quando o campo está vazio;
  - mensagem "Nenhum item corresponde a…".
- Abrir o resultado abre o arquivo ou a pasta pela montagem.

### 6.10 Preferências (equivalente à `preferences_dialog`)

- **Duas abas:** "Meu computador" (seção 6.5) e "Google Drive".
- **Aba "Google Drive":** modo Stream ou Espelho, local da pasta do Meu Drive e abrir a pasta.
- **Rodapé:** "Alterações pendentes", Cancelar e Salvar; aviso "algumas alterações exigem reiniciar",
  com o botão "Reiniciar agora".
- **Configurações avançadas:**
  - local do streaming, com mostrar no painel "Locais" do Dolphin;
  - pasta de cache (Alterar/Redefinir) e limite do cache (de 1 até o máximo);
  - "Ver armazenamento do cache";
  - iniciar ao fazer login (`AutoStartOnLogin`);
  - proxy: automático (do sistema), conexão direta ou manual;
  - limites de banda para download e upload, em KB/s;
  - atalho da pesquisa;
  - relatório automático de erros (**desligado por padrão**);
  - perguntar se quer fazer backup de dispositivos e lista de dispositivos ignorados;
  - pausar em conexão limitada;
  - tema (do sistema ou claro);
  - desconectar a conta.
- Itens definidos por política aparecem bloqueados com "Gerenciado pelo administrador".
- **Tour de boas-vindas** opcional.

### 6.11 Primeiro uso (equivalente à `onboarding_dialog`)

- **Passos em lista vertical:**
  1. entrar ou criar conta;
  2. aguardando o login, com "Gerar link";
  3. apresentação;
  4. "Veja seus arquivos no Dolphin", com botão para abrir;
  5. escolher pastas para sincronizar, com sugestões e tamanho;
  6. resumo;
  7. como tornar disponível off-line.
- **Botões:** Pular, Voltar e Continuar.
- **Política** `DisableOnboardingDialog`.

### 6.12 Integração com o gerenciador de arquivos (Dolphin primeiro)

- **Ícones de status** sobre os arquivos da montagem e das pastas conectadas: na nuvem, disponível
  off-line, sincronizando, bloqueado/não sincronizado e erro. O plugin `KOverlayIconPlugin` consulta o
  serviço por D-Bus.
- **Submenu "Google Drive"** no botão direito (`KAbstractFileItemActionPlugin`; o serviço devolve os
  itens conforme o item selecionado):
  - Abrir com o Google Drive (web) / Docs / Planilhas / Apresentações;
  - Compartilhar;
  - Copiar link, com notificação que diz se o link é público, do domínio ou restrito;
  - Acesso off-line: Disponível off-line ou Somente on-line, com marcação, inclusive mista;
  - Adicionar atalho ao Google Drive;
  - Gerenciar versões;
  - Ver lixeira;
  - Atualizar pasta;
  - Enviar ou sincronizar esta pasta (fora da montagem);
  - Adicionar ao espelho da conta 1–4;
  - Gerenciar drives compartilhados e Criar novo drive compartilhado (na pasta de drives);
  - Criar novo documento, planilha ou apresentação;
  - Perguntar ao Gemini (web).
- **Miniaturas sem baixar o arquivo:** um thumbnailer do KDE usa o `thumbnailLink` da API para
  arquivos que não estão no cache. No primeiro uso, o app **exclui as montagens do indexador Baloo**.
- Em outros gerenciadores de arquivos, o menu existe só como ações de serviço básicas (abrir na web,
  copiar link, off-line), sem ícones de status.

### 6.13 Compartilhamento, versões, lixeira e criação

- **Janela "Compartilhar" nativa** via `permissions`:
  - adicionar pessoas com papel (leitor, comentarista, editor) e mensagem;
  - acesso geral (restrito, domínio ou qualquer pessoa com o link);
  - copiar link.
  - Também abre pela ferramenta MCP, para corrigir a permissão de um e-mail.
- **"Gerenciar versões":** janela com as revisões do arquivo (`revisions`), com baixar, restaurar
  como atual e manter para sempre, quando a API permitir. Mensagens para "sem versões antigas" e
  "não foi possível obter as versões".
- **Lixeira:** pasta virtual somente leitura com os itens excluídos, e ação "Restaurar". Os itens
  são apagados para sempre pelo Drive após 30 dias.
- **Criar** Docs, Planilhas e Apresentações dentro de uma pasta da montagem: cria pela API e abre no
  navegador.

### 6.14 Notificações

- Notificações do sistema (`org.freedesktop.Notifications`) com ações, mais a lista na janela
  principal.
- Devem cobrir todas as famílias do original que se aplicam ao Linux:
  - erros fatais e de conta;
  - armazenamento e cota (quase cheio, cheio, disco local cheio);
  - sincronização e pastas (local mudou, pasta conectada sumiu, Lost and Found, itens
    recuperados, uploads parados há N dias, itens movidos para a lixeira);
  - permissões e movimentações (não pode criar, mover, excluir ou renomear; avisos de
    compartilhamento);
  - dispositivos (fazer backup?, seguro remover, backup concluído ou com erro);
  - "nova versão do app disponível", vinda das releases do GitHub, no lugar dos avisos do Google.
- Cada notificação opcional tem "Não mostrar de novo", e há uma tela "Preferências de
  notificação".

### 6.15 Diagnóstico, feedback e "Sobre"

- **Registro detalhado** liga/desliga; **Gerar informações de diagnóstico** cria um pacote de logs
  sem tokens nem conteúdo de arquivos.
- **Enviar feedback:** a mesma tela do original, mas o envio abre uma *issue* pré-preenchida no
  GitHub do projeto e, opcionalmente, anexa o pacote de diagnóstico, que o usuário revisa antes.
- **Relatório de falhas:** fica local e só é enviado com consentimento.
- **Sobre:** versão, licença GPL-3.0, softwares de terceiros, aviso de "não oficial", links do
  projeto.
- **Redefinir as preferências** e **redefinir os dados locais**, com confirmação.

### 6.16 Políticas e configuração

- **Mesmos nomes de chave do original**, em TOML, com três camadas e esta precedência:
  1. `/etc/gdrive-desktop-linux/policies.toml` (override do administrador);
  2. `~/.config/gdrive-desktop-linux/preferences.toml` (usuário);
  3. `/etc/gdrive-desktop-linux/defaults.toml` (padrão da máquina).
- **Chaves suportadas:**
  - `AllowedAccountsPattern`, `AutoStartOnLogin`, `BandwidthRxKBPS`, `BandwidthTxKBPS`;
  - `ContentCachePath`, `ContentCacheMaxKbytes`, `DefaultMountPoint`, `DefaultWebBrowser`;
  - `DisableAddAccount`, `DisableExternalMediaSync`, `DisableLocalizedVirtualFolders`,
    `DisableMirroredMyDrive`, `DisableMirroredFolders`, `DisableOnboardingDialog`;
  - `MinFreeDiskSpaceKBytes`, `OpenOfficeFilesInDocs`, `DirectConnection`, `TrustedRootCertsFile`,
    `TrustSystemRootCerts`, `DisableCRLCheck`, `DisableSSLValidation`;
  - `SearchHotKey`, `PauseSyncingOnMeteredConnection`, `AutomaticErrorReporting`, `UITheme`,
    `ShowMountPointInSidebar`, `PromptToBackupDevices`.
- A seção 3 lista as chaves que não se aplicam.

### 6.17 Servidor MCP

- Binário `gdrive-desktop-linux-mcp` (stdio, JSON-RPC), que fala com o serviço por D-Bus.
- **Mesmas 5 ferramentas:**
  - `get_sync_status(path)`;
  - `add_folder_to_sync(path, sync_mode, account_id?, my_drive_path?)`: em stream, move a pasta para
    o Meu Drive e deixa um link simbólico no lugar, com verificação e desfazer em cada etapa;
  - `get_drive_url(path)`;
  - `list_tracked_folders()`;
  - `open_acl_dialog(email, item_id, account_id?)`: só abre a janela de compartilhamento e nunca
    altera permissões sozinho.
- A regra do original vale aqui: um agente nunca altera permissões em silêncio.

## 7. Arquitetura (decisões tomadas)

| Peça | Decisão | Quem decidiu |
|---|---|---|
| Linguagem | **Rust** em tudo. A única exceção é o plugin do Dolphin, porque o KDE só aceita plugins em C++. | Dono do projeto (Rust); exceção: minha escolha |
| Motor | Serviço `gdrive-desktop-linuxd` (serviço systemd de usuário): `fuser` (FUSE 3), `tokio`, `rusqlite`, `reqwest` com rustls, `notify` (inotify), `keyring` (Secret Service) | Minha escolha |
| Comunicação | D-Bus na sessão (`zbus`): interface única do serviço usada pela interface, pelo plugin do Dolphin, pelo thumbnailer e pelo servidor MCP | Minha escolha |
| Interface | **Tauri 2** (WebKitGTK), porque as telas do original também são HTML num navegador embutido: é o caminho para a mesma aparência. Visual Material com fonte livre (ex.: Roboto Flex/Inter) e ícones Material Symbols (Apache-2.0). Tema claro/escuro do sistema. | Minha escolha |
| Bandeja | Processo leve próprio em Rust com `ksni` (o clique esquerdo funciona, o que não acontece no appindicator), sem WebKit. A janela Tauri só é carregada quando alguma tela é aberta e pode ser fechada sem afetar a bandeja nem o serviço. | Minha escolha |
| Modelo de dados | Um único banco SQLite por conta para Stream e Espelho, com a revisão-base por item desde a 0.1, para que o Espelho (0.4) não exija migração destrutiva | Minha escolha |
| Plugin do Dolphin | C++/Qt 6 pequeno: `KOverlayIconPlugin` + `KAbstractFileItemActionPlugin` + thumbnailer KIO. Toda a lógica fica no serviço. | Minha escolha |
| Idiomas | pt-BR e inglês desde o início, num sistema de tradução padrão (ex.: Fluent) | Minha escolha |
| Pacotes | RPM para Fedora/Nobara primeiro (repositório Copr); `.deb` depois. Flatpak fica fora, porque montagem FUSE e plugins do Dolphin não combinam com o sandbox. | Minha escolha |
| Dados locais | `~/.config/gdrive-desktop-linux/` (preferências), `~/.local/share/gdrive-desktop-linux/<conta>/` (banco e cache, ou `ContentCachePath`), `~/.local/state/gdrive-desktop-linux/` (logs) | Minha escolha |

Fluxo: a interface e os plugins **nunca** falam com o Google diretamente. Tudo passa pelo serviço,
que é a única fonte de verdade.

## 8. Roteiro (cada versão sai utilizável)

| Versão | Entrega |
|---|---|
| 0.1 | Serviço com login (CLI provisória), montagem Stream de 1 conta, metadados em SQLite, leitura sob demanda, gravação com fila, lixeira, Documentos Google como ponteiros, Disponível off-line (pela CLI) |
| 0.2 | Interface: bandeja, janela principal, primeiro uso, Preferências, notificações, pesquisa rápida, lista de erros, arquivos off-line |
| 0.3 | Dolphin: ícones de status, submenu completo, miniaturas, exclusão do Baloo; janelas de compartilhar e de versões |
| 0.4 | Modo Espelho, pastas conectadas, dispositivos removíveis e NAS, troca entre modos |
| 0.5 | Várias contas (até 4), Drives compartilhados gerenciáveis, políticas, ransomware, servidor MCP, diagnóstico e feedback |
| 1.0 | Paridade verificada contra o catálogo da seção 6, pacotes RPM e `.deb` |

Toda mudança atualiza o `CHANGELOG.md` e a versão (SemVer); o README é a página principal do
projeto e acompanha o roteiro.

## 9. Testes e qualidade

- Seguir a cadência de testes do dono do projeto: testes do que mudou durante o código; typecheck,
  lint e testes unitários antes de cada commit; suíte completa nas mudanças de risco; CI completo a
  cada push.
- **Testes do motor contra um Drive simulado** (servidor HTTP de mentira que imita a API v3,
  inclusive erros 403/429/5xx e `changes`), sem tocar em contas reais na CI.
- **Testes FUSE de verdade** numa montagem temporária, cobrindo leitura parcial, gravação, renomear,
  excluir, sem rede e reinício com fila pendente.
- **Testes de "nunca perder dados"** com falhas injetadas: queda de rede no meio do upload, processo
  morto, disco cheio, conflito.
- **Uma conta Google de teste**, só em testes manuais e de fumaça antes de cada release.
- **CI:** GitHub Actions hospedado, com `fuse3` instalado no runner Ubuntu para os testes FUSE. Se o
  runner hospedado não der conta, os testes FUSE vão para um contêiner privilegiado ou viram etapa
  manual antes da release. Os plugins do Dolphin compilam num contêiner Fedora com KF6.
- **Metas de desempenho** (verificadas por teste):
  - listar uma pasta de 10 mil itens já sincronizada em menos de 1 s, a partir do banco;
  - abrir a janela principal em menos de 1,5 s;
  - em repouso, serviço e bandeja juntos abaixo de 150 MB de RAM;
  - nenhuma leitura de conteúdo ao só navegar nas pastas pelo Dolphin, medida pelo contador de
    downloads do serviço.
- **Matriz de paridade:** uma lista de verificação gerada da seção 6, marcada a cada versão.

## 10. Deixado para quem constrói

- O formato exato do JSON dos ponteiros `.gdoc` e o tipo MIME registrado.
- O nome das cópias de conflito e o texto das mensagens, desde que fiéis ao sentido do original e sem
  copiar textos do Google.
- O tamanho dos blocos do cache, o paralelismo de downloads e o limiar do upload retomável (o
  original usa upload retomável a partir de 100 MiB).
- A ordem e os separadores dos menus (o levantamento não conseguiu a ordem exata do original).
- A heurística exata de ransomware e o intervalo padrão de consulta de mudanças (entre 10 e 30 s).
- A estrutura dos crates do workspace Rust.
