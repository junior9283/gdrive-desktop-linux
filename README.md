<p align="center">
  <img src="assets/icons/hicolor/128x128/apps/io.github.junior9283.GDriveDesktopLinux.png" alt="Ícone do gdrive-desktop-linux" width="128">
</p>

# gdrive-desktop-linux

![Versão](https://img.shields.io/badge/vers%C3%A3o-0.6.0-blue)
![Situação](https://img.shields.io/badge/situa%C3%A7%C3%A3o-pr%C3%A9via-orange)
![Licença](https://img.shields.io/badge/licen%C3%A7a-GPL--3.0-green)
![Linguagem](https://img.shields.io/badge/Rust-%E2%9C%93-b7410e)

Cliente **não oficial** do Google Drive para Linux, escrito em Rust, com o mesmo comportamento,
aparência e funcionalidades do Google Drive para computador (versão Windows).

> **English:** unofficial Google Drive desktop client for Linux, written in Rust, aiming at full
> feature and behavior parity with Google Drive for desktop on Windows. Contributions are welcome.

---

## Por que este projeto existe

O Google oferece o Google Drive para computador só para Windows e macOS. No Linux, as opções atuais
são montar o Drive com ferramentas genéricas de linha de comando ou usar só o navegador. Nenhuma
delas oferece a experiência do app oficial: instalar, entrar na conta e ter uma pasta em que os
arquivos ficam na nuvem até você abrir, com ícone na bandeja, "Disponível off-line" e backup de
pastas.

Este projeto quer preencher essa lacuna com um app **nativo de Linux**, feito do zero.

## O que já funciona

<p align="center">
  <img src="docs/imagens/janela-principal.png" alt="Janela principal: estado da sincronização e cartões de atenção, off-line, compartilhados, status e Gemini" width="720">
</p>

| Área | Situação |
|---|---|
| **Pasta do Drive** | ✅ Todo o Meu Drive em `~/Google Drive/Meu Drive`; os arquivos ficam na nuvem e são baixados só quando você abre (e só os trechos lidos). |
| **Off-line** | ✅ "Disponível off-line" em arquivos e pastas; as alterações feitas sem internet sobem quando a conexão volta. |
| **Sincronização** | ✅ Envio e recebimento automáticos, fila que sobrevive a reinícios, conflitos guardados no Lost and Found, lixeira, limites de banda e pausa. |
| **Documentos Google** | ✅ Docs, Planilhas e Apresentações aparecem na pasta e abrem no navegador com duplo clique. |
| **Bandeja e janela** | ✅ Ícone na bandeja, janela principal, atividade com progresso, pesquisa rápida, erros, arquivos off-line, preferências, notificações e menu da conta com foto e uso do espaço. |
| **Primeiro uso** | ✅ Guia do Client ID dentro do app, com botões que abrem cada página do Google Cloud. |
| **Miniaturas** | ✅ As imagens aparecem com miniatura no Dolphin e em outros gerenciadores, usando as miniaturas do próprio Google, sem baixar os arquivos. |
| **Menu do botão direito** | ✅ Abrir no navegador, copiar link, off-line / só on-line, novo Documento, Planilha ou Apresentação, atualizar e lixeira, no Dolphin, Nautilus, Nemo, Caja, Thunar e COSMIC Files. |
| **Instalação e remoção** | ✅ Pacotes `.deb`, `.rpm`, Arch e `.tar.gz`, e um desinstalador com a opção de apagar também os dados do computador. |
| **Ícones de status** | 🔜 Na nuvem, off-line e sincronizando, nos gerenciadores de arquivos. |
| **Backup e espelho** | 🔜 Backup de pastas do computador para o Drive e o modo espelho (cópia completa no computador). |
| **Contas** | 🔜 Várias contas Google ao mesmo tempo e Drives compartilhados. |

A lista completa e detalhada fica na especificação em [`docs/`](docs/).

### Telas

| Primeiro uso | Atividade de sincronização |
|---|---|
| ![Guia do Client ID no primeiro uso](docs/imagens/primeiro-uso.png) | ![Lista de envios e downloads com o status de cada um](docs/imagens/atividade.png) |
| **Menu da conta** | **Pesquisa rápida** |
| ![Menu da conta com a foto, o e-mail e o uso do espaço](docs/imagens/menu-da-conta.png) | ![Pesquisa rápida com três resultados](docs/imagens/pesquisa.png) |
| **Arquivos off-line** | **Preferências** |
| ![Uso do disco e itens off-line](docs/imagens/arquivos-off-line.png) | ![Aba Google Drive das preferências](docs/imagens/preferencias.png) |
| **Sobre** | **Desinstalar** |
| ![Tela Sobre com a versão e a licença](docs/imagens/sobre.png) | ![Janela que pergunta se apaga também os dados do computador](docs/imagens/desinstalar.png) |

## Como funciona

```
┌──────────────────────────┐      ┌───────────────────────────────┐
│ Interface (Tauri)        │      │ Gerenciadores de arquivos     │
│ bandeja, login,          │      │ (plugin do Dolphin, extensão  │
│ preferências, atividade  │      │  GTK: menu do botão direito)  │
└────────────┬─────────────┘      └───────────────┬───────────────┘
             │              D-Bus                 │
┌────────────┴────────────────────────────────────┴───────────────┐
│ Motor (serviço em Rust)                                          │
│  • pasta virtual via FUSE (arquivos sob demanda)                 │
│  • banco SQLite com os metadados de todo o Drive                 │
│  • cache em blocos (só os trechos lidos são baixados)            │
│  • fila de envios que sobrevive a reinícios e quedas de rede     │
│  • sincronização de mudanças e resolução de conflitos            │
└────────────────────────────────┬─────────────────────────────────┘
                                 │ HTTPS + OAuth 2.0
                        Google Drive API (pública)
```

### Diferenças inevitáveis em relação ao app oficial

- **Atualizações vindas da nuvem:** chegam em segundos, mas não na hora. O app oficial usa um canal
  de notificação interno do Google que não está disponível para terceiros.
- **Login:** nunca usa a identidade do app oficial. Nas versões 0.x, cada usuário cria o próprio
  Client ID no Google Cloud, seguindo o [guia passo a passo](docs/guia-client-id.md).
- **Backups de pastas:** a API pública não cria a seção "Computadores" do Drive. Os backups ficam em
  `Meu Drive/Computadores/<nome-do-computador>/`, e a pasta do app mostra "Computadores" como no
  original.
- **Sem o limite de 4 GB por arquivo** da unidade virtual do Windows: aqui o limite é o da nuvem.
- **Ficam fora:** presença em tempo real do Office, plugins do Outlook/Meet e backup para o Google
  Fotos (descontinuado pelo próprio Google).

A especificação completa está em
[`docs/superpowers/specs/2026-10-25-gdrive-desktop-linux-design.md`](docs/superpowers/specs/2026-10-25-gdrive-desktop-linux-design.md).

## Como instalar

É uma prévia para testes: funciona no dia a dia, mas mantenha cópia do que for importante.

**1. Instalar.** Baixe da [página de releases](https://github.com/junior9283/gdrive-desktop-linux/releases/latest)
o arquivo da sua distribuição (64 bits, x86_64):

| Distribuição | Arquivo | Como instalar |
|---|---|---|
| Ubuntu 22.04+, Debian 12+, Mint, Pop!_OS | `.deb` | `sudo apt install ./gdrive-desktop-linux_*.deb` |
| Fedora, Nobara, openSUSE | `.rpm` | `sudo dnf install ./gdrive-desktop-linux-*.rpm` (openSUSE: `sudo zypper install --allow-unsigned-rpm ./…rpm`) |
| Arch, Manjaro, EndeavourOS, CachyOS | `.pkg.tar.zst` | `sudo pacman -U ./gdrive-desktop-linux-*.pkg.tar.zst` |
| Qualquer outra | `.tar.gz` | instale o FUSE 3 e o WebKitGTK 4.1 da distribuição; depois `tar -xzf gdrive-desktop-linux-*.tar.gz` e `./gdrive-desktop-linux-*/install.sh` (instala em `~/.local`, sem root) |

O `SHA256SUMS` do release confere os arquivos baixados (`sha256sum -c --ignore-missing SHA256SUMS`).

<details>
<summary>Ou compilar o código</summary>

Dependências (Rust 1.99 ou mais novo, FUSE 3 e as bibliotecas da janela):

```bash
# Fedora/Nobara
sudo dnf install fuse3 webkit2gtk4.1-devel libsoup3-devel librsvg2-devel dbus-devel
# Debian/Ubuntu
sudo apt install fuse3 libwebkit2gtk-4.1-dev librsvg2-dev libdbus-1-dev build-essential
```

Compilar e instalar para o seu usuário, sem root (em `~/.local`):

```bash
scripts/install-dev.sh                 # compila em release e instala
scripts/install-dev.sh --dry-run       # só mostra o que instalaria
scripts/install-dev.sh --prefix /outro/lugar
```

Ele instala os quatro programas (serviço, CLI, bandeja e janela), o serviço do usuário (systemd),
a ativação do serviço e da janela pelo D-Bus, o atalho no menu de aplicativos, a abertura dos
ponteiros `.gdoc`, os ícones e a extensão do menu do botão direito para Nautilus, Nemo, Caja e
Thunar. Os plugins do Dolphin (menu e emblemas de estado) compilam à parte
(`integrations/dolphin/build.sh`, com CMake e KF6) e vão na pasta de plugins do Qt do sistema.

`scripts/package.sh dist` gera os mesmos instaladores do release (precisa do
[nfpm](https://nfpm.goreleaser.com)).

</details>

**Desinstalar:** pelo item **Desinstalar Google Drive** do menu de aplicativos ou pelo botão
**Desinstalar…** em Mais opções › Sobre. A janela pergunta se apaga também os dados deste
computador (conta conectada, cache, arquivos off-line, miniaturas e preferências; as cópias do Lost
and Found ficam) e, se o app veio de um pacote, o sistema pede a senha de administrador. Pela loja
ou pelo terminal (`apt remove`, `dnf remove`, `pacman -R`) só os programas saem e os dados ficam,
como é o padrão no Linux; quem instalou pelo `.tar.gz` também pode rodar `./uninstall.sh`.

**2. Abrir:** procure **Google Drive** no menu de aplicativos (ou rode
`gdrive-desktop-linux-tray`). A bandeja liga o serviço e, sem conta, abre o primeiro uso, que
guia a criação do seu Client ID parte por parte, com botões que abrem cada página do Google Cloud
e copiam os valores (o mesmo passo a passo do [guia](docs/guia-client-id.md)). Depois, cole o
Client ID e o segredo e entre pelo navegador. A pasta aparece em `~/Google Drive/Meu Drive`.

Na bandeja: estado da sincronização, pausar/retomar, pesquisa (também pelo atalho global, se o
portal do KDE aceitar), lista de erros, arquivos off-line, preferências e sair. A janela
principal tem o Início, a atividade com progresso e "Cancelar envio", os compartilhados comigo e
o histórico de notificações.

**Menu do botão direito:** nos arquivos e pastas do Meu Drive aparece o submenu **Google Drive**,
com abrir no navegador, copiar link, Disponível off-line / Somente on-line (marcado conforme o
estado atual), novo Documento, Planilha ou Apresentação Google na pasta, atualizar a pasta e ver
a lixeira. Fora do Meu Drive ele não aparece.

| Gerenciador | Como funciona |
|---|---|
| Dolphin (KDE Plasma 6) | plugin que vem nos pacotes `.deb`, `.rpm` e Arch (no `.tar.gz` não: o Qt só procura plugins nas pastas do sistema) |
| Nautilus (GNOME, Ubuntu, Pop!_OS 22.04, Zorin), Nemo (Cinnamon, Mint), Caja (MATE), Thunar (Xfce) | extensão que vem em todos os formatos; precisa do pacote de Python do gerenciador: `nautilus-python`/`python3-nautilus`, `nemo-python`/`python3-nemo`, `python3-caja`/`python-caja` ou `thunarx-python` |
| COSMIC Files (Pop!_OS 24.04) | não aceita extensões nem submenus: o serviço põe as ações soltas, com "Google Drive:" no nome, e elas avisam quando o item não é do Meu Drive |

Também há uma CLI:

| Comando | O que faz |
|---|---|
| `login` / `status` | entra na conta / conta, pasta, envios pendentes |
| `pin <caminho>` / `unpin <caminho>` | deixa disponível off-line / só on-line |
| `pause` / `resume` | pausa e retoma a sincronização |
| `lost-and-found` | lista as edições guardadas por conflito |
| `logout [--finish \| --move <pasta> \| --discard]` | sai da conta, decidindo o que fazer com envios pendentes (o Lost and Found nunca é apagado) |
| `open-pointer <arquivo>` | abre um `.gdoc`, `.gsheet`… no navegador (usado pelo duplo clique) |
| `menu-action <ação> <caminho>…` | faz uma ação do menu do botão direito (usado pelo COSMIC Files) |
| `uninstall [--purge] [--yes]` | desinstala o app; `--purge` apaga também os dados deste computador (desconecte a conta antes) |

Limitações conhecidas:

- **Vim e Neovim** (com o padrão `backupcopy=auto`) salvam renomeando o original e criando outro:
  o conteúdo não se perde, mas no Drive o arquivo ganha um ID novo e o antigo vai para a lixeira
  (links compartilhados do antigo deixam de valer). Com `set backupcopy=yes` o ID se mantém.
- Uma pasta criada aqui que ainda não subiu, mandada para a lixeira, some só deste computador.
- Ainda sem os ícones de status no gerenciador de arquivos, backup de pastas, modo espelho e várias
  contas (veja o roteiro abaixo).

Roteiro de teste manual (e o que já foi conferido): [docs/testes](docs/testes/README.md).

## Roteiro

Cada etapa sai funcionando antes da próxima.

| Versão | Etapa |
|---|---|
| 0.0.x | Design e especificação |
| 0.1 | Motor: pasta virtual, cache, sincronização e modo off-line |
| 0.2 | Bandeja, janela, primeiro uso com login e preferências |
| 0.3 | Miniaturas do Google no gerenciador de arquivos |
| 0.4 | Instaladores para as distribuições e desinstalador |
| 0.5 | Menu do botão direito em todos os gerenciadores |
| 0.6 | Desempenho: CPU parada, pastas grandes, download e envios em paralelo ← **estamos aqui** |
| 0.7 | Dolphin completo: ícones de status, janelas de compartilhar e de versões |
| 0.8 | Várias contas e Drives compartilhados |
| 0.9 | Backup de pastas, pastas conectadas e modo espelho |
| 0.10 | Diagnóstico e feedback, notificações restantes, servidor MCP, políticas restantes |
| 1.0 | Paridade com a versão Windows, item a item na [matriz de paridade](docs/paridade.md) |

## Versionamento

O projeto segue o [Versionamento Semântico](https://semver.org/lang/pt-BR/) (`MAIOR.MENOR.CORREÇÃO`).
Toda mudança é registrada no [CHANGELOG](CHANGELOG.md), e cada versão publicada ganha uma tag
`vX.Y.Z`.

## Como contribuir

Contribuições são bem-vindas, seja em código, testes, tradução, documentação ou relatos de uso.
A melhor forma de ajudar agora é testar a prévia e abrir uma *issue* com o que deu errado ou com
sugestões. Antes de mandar código, veja como rodar os testes em [docs/testes](docs/testes/README.md).

Uma regra vale para todos: **nenhum código, texto, ícone, fonte ou outro arquivo do Google** pode
entrar no repositório. O comportamento do app oficial pode ser estudado e descrito, mas a
implementação tem que ser própria.

## Perguntas frequentes

**É oficial?** Não. O projeto não tem vínculo com o Google.

**É seguro?** O app só acessa a sua conta pela API pública do Google, com a sua autorização
(OAuth), e o código é aberto para qualquer um conferir.

**Por que Rust?** Pela segurança de memória e pelo desempenho num serviço que fica rodando o tempo
todo e lida com os seus arquivos.

## Aviso legal

Este projeto não tem vínculo com o Google LLC nem é endossado por ele. "Google Drive" e "Google
Fotos" são marcas registradas do Google LLC e são citadas aqui apenas para indicar compatibilidade.

## Licença

[GNU GPL v3.0](LICENSE): você pode usar, estudar, modificar e redistribuir o app. Versões
modificadas distribuídas também precisam ter o código aberto.
