# Atualização automática — desenho

Data: 2026-11-07. Ponto de partida: versão 1.3.2.

## Intenção

Hoje o serviço já descobre que saiu versão nova (`crates/gdl-daemon/src/updates.rs` e
`daemon/releases.rs`: consulta `LATEST_RELEASE_API` 1 min depois da partida e a cada 24 h) e
mostra uma notificação com "Ver novidades", que só abre a página do release. Para atualizar, a
pessoa precisa baixar o pacote certo e instalar no terminal. O objetivo é que o próprio app ofereça
**Atualizar agora** ou **Depois** e, ao escolher atualizar, baixe, confira, instale e volte a rodar
já na versão nova, sem terminal.

## Metas

1. **Onde aparece a oferta** (decisão do usuário: os três lugares):
   - **notificação do sistema**, com dois botões: **Atualizar agora** e **Depois**;
   - **janela do app, tela Sobre** (`crates/gdl-ui/ui/screens/about.js`): enquanto houver versão
     nova, uma faixa "Versão X.Y.Z disponível" com **Atualizar agora** e "Ver novidades";
   - **menu da bandeja** (`crates/gdl-tray/src/model.rs`): item "Atualizar para X.Y.Z" enquanto
     houver versão nova (nos dois menus, conectado e desconectado).
2. **"Depois"** (decisão do usuário): só fecha a notificação. A mesma versão volta a ser oferecida
   na consulta seguinte (24 h) até ser instalada. A tela Sobre e a bandeja continuam mostrando a
   oferta o tempo todo. Isso troca o "avisa uma vez por versão" de hoje (`versao-avisada`) por
   "avisa a cada consulta enquanto não instalar"; o teste `newer_release_is_announced_once` muda
   junto. Reiniciar o serviço não deve gerar aviso extra antes das 24 h (guardar a hora do
   último aviso, não só a versão).
3. **"Atualizar agora"** (decisão do usuário: baixar e instalar sozinho):
   1. descobre a **variante** (`brand::PACKAGE`: `gdrive-desktop-linux` ou `gerasync`) e o
      **formato da instalação** (ver "Como o app sabe como foi instalado");
   2. baixa do release, para uma pasta privada em `<cache_dir>/atualizacao/`, o `SHA256SUMS`, a
      assinatura `SHA256SUMS.minisig` e o pacote da variante e do formato;
   3. **confere a assinatura** do `SHA256SUMS` com a chave pública embutida no app e depois o
      SHA-256 do pacote contra a linha dele; qualquer falha para tudo, sem instalar;
   4. instala:
      | Formato | Comando |
      |---|---|
      | `.deb` | `pkexec apt-get install -y <caminho absoluto>.deb` |
      | `.rpm` | `pkexec dnf install -y <rpm>`; sem `dnf`, `pkexec zypper --non-interactive install --allow-unsigned-rpm <rpm>` |
      | Arch | `pkexec pacman -U --noconfirm <pkg.tar.zst>` |
      | `.tar.gz` | extrai e roda o `install.sh` dele com `--prefix` igual ao da instalação atual, sem root |
      A senha é pedida pela janela gráfica do polkit; nunca no terminal nem com `sudo`.
   5. reinicia o serviço (`systemctl --user restart <DAEMON>.service`), a bandeja e a janela
      aberta, para que tudo rode a versão nova sem sair da sessão;
   6. o serviço novo, ao partir, vê que veio de uma atualização (arquivo de estado gravado antes
      do passo 4 com a versão alvo) e notifica **"Atualizado para X.Y.Z"**, com "Ver novidades".
4. **Progresso visível**: enquanto baixa e instala, a notificação (mesma `replaces_key`) e a tela
   Sobre mostram o passo atual (baixando / aguardando a senha / instalando). Um clique repetido em
   "Atualizar agora" durante uma atualização não começa outra.
5. **Falhas são seguras e explicadas**: sem rede, download incompleto, assinatura ou soma que não
   conferem, senha cancelada, gerenciador de pacotes com erro (ex.: outro `apt` rodando). Em todos
   os casos: notificação curta com o motivo, nada instalado pela metade por nossa conta, arquivos
   baixados apagados, e a oferta continua disponível para tentar de novo. Cancelar a senha não é
   erro: volta ao estado "versão disponível" em silêncio.
6. **Textos em todos os idiomas** de `locales/`, como o resto do app.

## Assinatura dos releases (decisão do usuário)

Os pacotes não são assinados hoje e o `SHA256SUMS` vem do mesmo release, então sozinho ele só
protege contra download corrompido. Como a atualização roda como root, o release passa a ser
assinado:

- uma chave **minisign (Ed25519)** criada uma vez; a secreta (e a senha dela, se houver) vai para
  os segredos do repositório no GitHub; a pública fica embutida no código (`gdl-common`, ao lado de
  `links.rs`) e documentada no README para quem quiser conferir à mão;
- o job `publish` de `.github/workflows/release.yml` assina o `SHA256SUMS` final (já com as linhas
  das duas variantes) e publica `SHA256SUMS.minisig` junto;
- o app verifica com uma biblioteca só de verificação (ex.: crate `minisign-verify`), sem chamar
  programas externos;
- release sem `SHA256SUMS.minisig` ou com assinatura inválida **nunca** é instalado: a oferta vira
  só "Ver novidades" (abre a página) e o log registra o motivo.

Criar a chave e cadastrar os segredos depende do usuário (posso gerar o par localmente e passar o
comando `gh secret set` para ele rodar; a chave secreta nunca vai para o repositório nem para log).

## Como o app sabe como foi instalado

Proposta: um arquivo marcador instalado junto com os programas, em
`<prefixo>/share/<PACKAGE>/instalacao`, com uma palavra:

- `deb`, `rpm` ou `arch`: gravado por `scripts/package.sh`/`packaging/nfpm.yaml` (o nfpm permite
  conteúdo por empacotador com `packager:`);
- `tar`: gravado pelo `install.sh` do `.tar.gz`;
- ausente ou `fonte`: instalação feita do código-fonte (`scripts/install-dev.sh` direto).

O prefixo é deduzido do caminho do programa em execução (`<prefixo>/bin/<DAEMON>`). O construtor
pode trocar o mecanismo (ex.: perguntar a `dpkg -S`/`rpm -qf`/`pacman -Qo`) se achar mais
robusto, desde que o resultado seja o mesmo.

O nome do arquivo no release segue o que `scripts/package.sh` gera hoje (`<pkg>_<v>_amd64.deb`,
`<pkg>-<v>-1.x86_64.rpm`, `<pkg>-<v>-1-x86_64.pkg.tar.zst`, `<pkg>-<v>-x86_64.tar.gz`). O app pega
o endereço em `assets[].browser_download_url` da mesma resposta da API, aceitando só o nome exato
esperado.

## Não-metas

- **Instalação do código-fonte** (`install-dev.sh` sem marcador): não instala sozinho; "Atualizar
  agora" abre a página do release. *(decisão minha)*
- **Arquiteturas fora de x86_64** e formatos que não publicamos (Flatpak, Snap, AUR): mesma regra,
  abre a página. *(decisão minha)*
- **Atualizar sem perguntar** (instalação silenciosa em segundo plano): não. Sempre há o clique em
  "Atualizar agora" e a senha do sistema.
- **Voltar para versão anterior** pelo app.
- **Quem está no 1.3.2 ou antes**: recebe o aviso antigo e instala a versão com atualização
  automática à mão, uma única vez. Isso fica dito nas notas do release.

## Anti-metas (falharia mesmo "funcionando")

- Instalar qualquer coisa cuja assinatura não foi conferida.
- Pedir senha no terminal, usar `sudo` ou guardar senha.
- Deixar o app parado (serviço fora do ar, bandeja sumida) depois de uma atualização bem-sucedida
  ou de uma falha.
- Insistir: mais de um aviso por consulta, ou aviso a cada partida do serviço.
- Trocar de variante (instalar GeraSync em quem usa gdrive-desktop-linux ou o contrário).

## Restrições

- O formato do código segue o projeto: comentários e mensagens em pt-BR, `cargo fmt --all` antes
  do commit (o CI exige), testes do serviço com o `Harness` e o `gdl-fake-drive`
  (`set_latest_release` hoje).
- O `gdl-fake-drive` passa a servir também os arquivos do release (pacote, `SHA256SUMS`,
  `.minisig`), para testar download, assinatura válida/inválida e soma errada sem rede.
- A execução do instalador fica atrás de uma interface que os testes substituem (nenhum teste
  chama `pkexec` de verdade).
- Nada de segredo em log; o log registra versão, formato, passo e erro.

## Ponto técnico a conferir antes de fixar o desenho do código

O serviço roda como unidade do systemd do usuário, fora da sessão gráfica. É preciso confirmar,
no Nobara/KDE e no GNOME, que um `pkexec` disparado por ele encontra o agente do polkit da
sessão e mostra a janela de senha. **Se não encontrar**, quem roda o passo 4 (só ele) é a janela
do app (`gdl-ui`, que vive na sessão): o serviço baixa e confere, abre a janela numa tela de
"Atualizando" e ela executa o comando. O construtor decide qual caminho com base nesse teste.

## Respostas à revisão do construtor (decisões minhas, salvo indicação)

1. **`pkexec` do serviço ou da janela.** A primeira tarefa do plano é o teste real nesta máquina
   (Nobara/KDE): `systemd-run --user --wait pkexec true`, com o usuário digitando a senha. O resto do
   plano fica bloqueado até o resultado. Se a janela tiver que executar, o serviço abre a janela na
   tela Sobre só para isso (aceitável), passa o caminho do pacote já conferido por D-Bus e a janela
   roda o comando. O reinício (item 2) continua sendo feito pelo serviço. GNOME fica conferido no
   uso, não bloqueia.
2. **Reinício sem matar quem atualiza.** Os pacotes não têm scriptlets (`packaging/nfpm.yaml` não
   define nenhum), então nada reinicia sozinho. Ordem: grava o estado `atualizando-para` → instala
   → em caso de sucesso, `systemctl --user --no-block restart <DAEMON>.service` (o systemd mata e
   sobe o serviço; o novo lê o estado e avisa o passo 6). A bandeja e a janela notam o serviço
   novo, com versão diferente da delas, e se reexecutam. No `.tar.gz`, o `install.sh` usa
   `install -D`, que troca o arquivo (unlink + novo) sem corromper o binário em execução.
3. **Chave minisign.** Os testes usam um par de teste nas fixtures, e a chave pública é injetável
   pela configuração do serviço (como o `UpdateCheck.url`), com a real como padrão. A chave real
   é uma **etapa manual do usuário no fim do plano**: gerar o par, rodar `gh secret set` e trocar
   a constante. Sem isso, o release 1.4.0 não pode sair.
4. **Release sem assinatura.** Se `SHA256SUMS.minisig` não estiver em `assets[]`, "dá para instalar
   sozinho" é falso já na consulta, e a oferta vira "Ver novidades". A validade da assinatura é
   conferida só no download. Se falhar, mostra o aviso de falha (meta 5) e abre a página.
5. **Prefixo.** `std::env::current_exe()` canonicalizado → `<prefixo>/bin/<DAEMON>` →
   `<prefixo>/share/<PACKAGE>/instalacao`. Sem marcador ao lado do binário (ex.: pasta movida),
   trata como `fonte` e abre a página.
6. **RPM.** O CI já instala o `.rpm` não assinado com `dnf install ./…rpm` no Fedora, o que mostra
   que `localpkg_gpgcheck` não bloqueia. O `--allow-unsigned-rpm` do zypper é intencional (a
   assinatura minisign cobre o pacote). Sem `dnf` nem `zypper`, abre a página.
7. **Estado e notificação.** Um arquivo novo (`atualizacao.json`: versão e hora do último aviso)
   substitui `versao-avisada`, aproveitando a versão do antigo, se existir. Se o servidor de
   notificações não suportar botões, a oferta continua na bandeja e na tela Sobre. Clicar em
   "Atualizar agora" na notificação não abre a janela (exceto no caso do item 1).

## O que fica com o construtor

- O formato das chamadas novas no D-Bus (`gdl-ipc`): algo como `UpdateInfo` (versão disponível,
  se dá para instalar sozinho, passo atual) + `StartUpdate` + um sinal de mudança, para a bandeja e
  a janela; e como a bandeja e a janela se reiniciam depois que o serviço volta na versão nova.
- Suporte a dois botões na `Notification` (hoje há uma única `action`).
- Onde exatamente a faixa entra na tela Sobre e o texto final de cada mensagem.
- Número da versão que sai com isso (sugestão: 1.4.0), CHANGELOG, README (seção de instalação e
  a chave pública) e `docs/index.md`.
