# GeraSync: a variante verificada pelo Google

**Data:** 04/11/2026 · **Situação:** desenho para aprovação

## Por que

Hoje, quem instala o gdrive-desktop-linux precisa criar um projeto no Google Cloud e colar o próprio
Client ID antes do primeiro login (`docs/guia-client-id.md`). É o passo mais chato do app, e foi o
assunto da conversa no fórum do Diolinux com o autor do DriveSync (rafatosta), que embute um Client
ID oficial no build.

O DriveSync conseguiu só a **verificação da marca** (nome e logotipo na tela de login). O próprio
README dele diz que o escopo `drive` segue pendente, com o aviso de "app não verificado" e o teto de
100 contas. O objetivo aqui é ir além: a **verificação completa** (marca + escopo restrito `drive`),
para que qualquer pessoa instale, entre e use, sem aviso e sem teto.

Para não afetar o projeto atual, isso vira uma **segunda variante** do mesmo código: o **GeraSync**.

## Objetivos

1. Cada release publica dois conjuntos de instaladores (.deb, .rpm, .pkg.tar.zst, .tar.gz), e a
   pessoa escolhe qual baixar:
   - **gdrive-desktop-linux**, exatamente como hoje (cada usuário traz o próprio Client ID);
   - **GeraSync**, com o Client ID oficial embutido no build pelo GitHub Actions.
2. As duas variantes podem ficar instaladas e rodando ao mesmo tempo sem se tocar: pastas, dados,
   configurações, chaveiro, serviço, nomes no D-Bus, bandeja, atalhos e integrações separados.
3. O GeraSync tem identidade própria: nome, ícone original e site próprio.
4. Tudo o que o Google pede para verificar marca e escopo fica pronto para o envio: site, política,
   termos, justificativa do escopo, roteiro do vídeo e checklist do Console.

## Fora do escopo

- Mudar o comportamento, a interface ou o fluxo de login da variante atual.
- Renomear o repositório `junior9283/gdrive-desktop-linux` ou o site atual.
- Trocar o escopo `drive` por um menor: o app espelha o Drive inteiro e precisa dele.
- Publicar em lojas (Flathub etc.).

## Anti-objetivos (seria fracasso mesmo funcionando)

- Qualquer mudança perceptível para quem usa a variante atual (pastas, contas, IDs, atalhos).
- O GeraSync ler ou alterar dados, contas ou a pasta da variante atual, ou o contrário.
- O Client ID ou o segredo oficiais entrarem em arquivo commitado, em log ou em artifact público
  além do próprio binário.
- O GeraSync parecer um produto do Google: nada de "Google" ou "GDrive" no nome, nada de logotipo
  parecido com o do Drive, nada de texto dizendo que é "igual ao app do Google".

## Decisões do usuário

| Assunto | Decisão |
|---|---|
| Caminho | Verificação real (marca + escopo `drive`), para romper o teto de 100 contas |
| Distribuição | Dois tipos de pacote no mesmo release; a pessoa escolhe qual baixar |
| Nome | **GeraSync**. Na tela do Google: "GeraSync for Google Drive". (Existe um gerasync.com de automação comercial, em "Coming Soon"; o usuário aceitou o risco.) |
| Repositório | Continua `gdrive-desktop-linux`, com o mesmo código para as duas variantes |
| Site | Repositório próprio, publicado em `junior9283.github.io/gerasync/` |
| Domínio | `junior9283.github.io`, com propriedade confirmada no Search Console. Se o Google recusar, troca-se por domínio próprio. |
| Convivência | Pastas e dados separados; as duas rodam juntas |
| Ícone | Novo e original, criado por mim; 2 ou 3 variações mostradas no navegador para o usuário escolher |
| E-mail público (suporte, contato, política) | `gerasync@googlegroups.com`, um Grupo do Google ("GeraSync Support") criado em 04/11/2026 na conta do usuário. O e-mail de suporte do Console só aceita a conta logada ou um grupo gerenciado por ela. Qualquer pessoa pode enviar mensagem, só os participantes veem as conversas, e cada mensagem chega no Gmail do usuário. |
| Repositório do site | Eu crio `junior9283/gerasync`, envio o site e ligo o Pages pelo `gh`, depois de mostrar o conteúdo pronto ao usuário |

## Decisões minhas (para o usuário conferir)

1. **Credenciais no build.** O build do GeraSync só aceita credenciais falsas com uma variável
   explícita (ex.: `GERASYNC_ALLOW_FAKE_CREDS=1`), usada pelo CI de push e de PR. Isso inclui os
   PRs de fork, que não recebem segredos. No `release.yml` de tag, a variável não existe, e um passo
   logo no início confere se os segredos estão presentes e não são os falsos. Assim, um release
   nunca sai com credencial de teste e a falta de segredo derruba o job em segundos.
2. **Critério de "build atual idêntico".** Os arquivos que o pacote da variante atual instala
   (caminhos e conteúdo de `.desktop`, `.service`, D-Bus, MIME, plugins, ícones) saem idênticos aos
   de hoje. Um teste compara a lista e o conteúdo com os da v1.2.3. Templates gerados são permitidos
   desde que esse teste passe. Os pacotes não declaram `Conflicts:` entre si: todo caminho é
   parametrizado, então os dois se instalam juntos, inclusive pelo mesmo gerenciador.
3. **Tauri.** O `identifier` e o `productName` do GeraSync vêm de uma configuração do Tauri gerada
   ou mesclada no build. O `tauri.conf.json` atual não muda.
4. **Textos da interface.** Textos que se referem ao *serviço* do Google continuam ("Meu Drive",
   "Google Drive", "Drives compartilhados"). Textos que dão nome ao *app* passam a dizer "GeraSync":
   título da janela, bandeja, notificações, item do menu do botão direito, "Sobre", o lugar na
   barra lateral do gerenciador de arquivos e a pasta `~/GeraSync`. O texto que diz que o app é
   "igual ao do Google" não aparece no GeraSync.
5. **Login no GeraSync.** Os campos e comandos de Client ID ficam fora da interface e da CLI do
   GeraSync, e qualquer Client ID achado na configuração é ignorado: vale sempre o embutido.
6. **Ícone provisório.** O código e o empacotamento usam um ícone provisório marcado como tal até
   o usuário escolher o definitivo, para o trabalho não parar esperando.
7. **Falha no release.** Como hoje, o release só publica se todos os jobs passarem. A checagem de
   segredos no início evita que o GeraSync derrube o release tarde. Os testes de instalação por
   distribuição dobram, e isso é aceito.
8. **Política e termos.** Redigidos por mim a partir de `docs/politica-de-privacidade.md` e
   `docs/termos-de-uso.md`, adaptados ao GeraSync e ao texto de Limited Use do Google. Não substituem
   revisão jurídica, e o site não diz que substituem.

## Como fica

### Identidade num lugar só

Hoje o nome e os IDs estão espalhados (`paths.rs`, `token_store.rs`, `dbus.rs`, `desktop.rs`,
`mount.rs`, `links.rs`, `tauri.conf.json`, `packaging/`, `integrations/`, `scripts/`). O builder
concentra a identidade numa fonte única (ex.: um módulo `brand` em `gdl-common`) escolhida **na
compilação**. Todo o código e todo o empacotamento passam a ler dessa fonte. Sem nada definido, o
build é o atual, byte a byte nos nomes.

O que muda por variante:

| Item | gdrive-desktop-linux (atual) | GeraSync |
|---|---|---|
| Nome exibido | gdrive-desktop-linux / "Google Drive" nos menus | GeraSync |
| ID do app (desktop, D-Bus, ícone) | `io.github.junior9283.GDriveDesktopLinux` | `io.github.junior9283.GeraSync` |
| Nome do pacote e dos binários | `gdrive-desktop-linux*` | `gerasync*` |
| Pastas XDG (config, data, state, `/etc`) | `gdrive-desktop-linux` | `gerasync` |
| Atributo do chaveiro | `gdrive-desktop-linux` | `gerasync` |
| Ponto de montagem | `~/Google Drive` | `~/GeraSync` |
| Serviço systemd do usuário | `gdrive-desktop-linuxd.service` | `gerasyncd.service` |
| Plugins do Dolphin e extensão GTK | nomes atuais | nomes próprios, falando com o D-Bus do GeraSync |
| Ícone | atual | novo |
| Links "sobre/ajuda/privacidade" | site atual | `junior9283.github.io/gerasync/` |
| Client ID | o usuário informa (guia) | embutido no build |

### Login no GeraSync

- O primeiro uso pula o guia do Client ID e vai direto para "Entrar com o Google".
- O Client ID e o segredo oficiais vêm de **segredos do repositório no GitHub** (ex.:
  `GERASYNC_CLIENT_ID`, `GERASYNC_CLIENT_SECRET`), lidos no build. O segredo de app desktop não é
  confidencial de verdade (vai no binário), mas não deve ficar em arquivo commitado nem em log.
- Sem os segredos, o build do GeraSync **falha**, em vez de gerar um pacote que não loga.
- Fora isso, o login é o mesmo: loopback `127.0.0.1`, PKCE, tokens só no chaveiro, os mesmos três
  escopos (`openid email https://www.googleapis.com/auth/drive`).
- **Decisão minha:** o GeraSync não oferece "usar meu próprio Client ID". Quem quiser isso usa a
  variante atual.

### Build e release

- O `release.yml` gera as duas variantes a cada tag `v*`, com a mesma versão. Os testes de
  instalação por distribuição valem para as duas.
- Os nomes dos arquivos deixam claro qual é qual (`gdrive-desktop-linux-1.3.0…` e
  `gerasync-1.3.0…`), com os dois no `SHA256SUMS`.
- O CI de cada push compila e testa as duas identidades, sem os segredos (o GeraSync de teste usa
  credenciais falsas), para que nenhuma mudança quebre só uma delas.
- As notas do release explicam a diferença numa linha.

### Site do GeraSync (repositório `junior9283/gerasync`)

- Uma página inicial em inglês, com versão em português, que diz o que o app faz, por que precisa de
  acesso ao Drive e que os dados vão só do computador para o Google, sem servidor no meio. O nome
  **GeraSync** aparece no título e no cabeçalho, igual ao da tela de login (a causa mais comum de
  recusa é nome diferente entre tela e página).
- Política de privacidade e termos no mesmo domínio, com link na página inicial, cobrindo o que a
  política de **Limited Use** do Google exige.
- O aviso "GeraSync is not affiliated with Google", link para download (releases) e contato.
- A meta tag de verificação do Search Console, que o usuário gera e eu coloco.

### Pacote do pedido ao Google (`docs/verificacao-google/` no repositório atual)

- **Checklist do Console**, na ordem: projeto de produção → Drive API → Branding (nome, logotipo,
  e-mail, página inicial, política, termos, domínio autorizado) → Search Console → Acesso a dados
  (três escopos) → cliente "App para computador" → segredos no GitHub → publicar → verificação da
  marca → verificação do escopo.
- **Justificativa do escopo `drive`** em inglês: por que `drive.file` e `drive.readonly` não bastam
  (espelho e montagem do Drive inteiro, envio de alterações locais e arquivos já existentes que o app
  não criou).
- **Roteiro do vídeo** em inglês: instalar o GeraSync, tela de consentimento em inglês com o Client ID
  visível na URL, os escopos pedidos e cada uso do escopo na prática.
- **Resposta sobre o CASA** em inglês: o app não tem servidor, e os dados ficam entre o computador e o
  Google. Pela documentação, a avaliação só é exigida quando os dados passam por servidores. Isso é
  leitura da regra, não garantia; a decisão é do Google.
- Lembrete da reverificação anual.

Quem faz o quê: eu preparo código, build, site, textos e roteiro, e guio o usuário no Console pelo
Chrome. Os cliques de envio no Google e a gravação do vídeo são do usuário.

## Restrições

- Rust, Tauri e nfpm, como hoje; nada de dependência nova sem necessidade.
- A cadência de testes do projeto continua valendo (`simplify` antes dos testes; `cargo fmt --all`
  antes do commit). A mudança é estrutural e mexe com auth, então pede a suíte completa no fim.
- Os testes de empacotamento (`crates/gdl-common/tests/packaging.rs`) passam a cobrir as duas
  variantes, incluindo a prova de que nenhum nome de uma vaza para a outra.
- Push só com pedido explícito; a cada push, README, `docs/index.md` e capturas atualizados.

## Deixado para o builder

- O mecanismo exato da fonte única de identidade (variável de ambiente no build, feature do Cargo ou
  outro), desde que o build padrão continue idêntico ao atual.
- Como o empacotamento (`install-dev.sh`, `package.sh`, `nfpm.yaml`, arquivos `.desktop`, `.service`,
  D-Bus) recebe os nomes: templates, geração ou substituição.
- A organização interna do site (gerador estático ou HTML puro).

## Riscos

- **Domínio `github.io` recusado:** nesse caso, comprar um domínio e apontar o site para ele.
- **Recusa por nome diferente entre a tela e a página:** mitigado pelo título e cabeçalho iguais; há
  relatos de recusas repetidas mesmo assim.
- **CASA exigido:** custo e prazo extras, decididos pelo Google.
- **Prazo:** a marca sai em dias; o escopo restrito costuma levar semanas.
- **Nome GeraSync:** existe uma empresa com esse domínio, em outro ramo.
