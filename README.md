# gdrive-desktop-linux

![Versão](https://img.shields.io/badge/vers%C3%A3o-0.1.0-blue)
![Situação](https://img.shields.io/badge/situa%C3%A7%C3%A3o-motor%20(pr%C3%A9via)-orange)
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

## Funcionalidades previstas

O objetivo é ter tudo o que a versão Windows oferece. A lista completa e detalhada fica na
especificação em [`docs/`](docs/).

| Área | O que o app vai fazer |
|---|---|
| **Pasta do Drive** | Uma pasta com todo o seu Drive, em que os arquivos são baixados só quando você abre (arquivos sob demanda). Também haverá o modo espelho, com cópia completa no computador. |
| **Off-line** | Marcar arquivos e pastas como "Disponível off-line" e trabalhar sem internet. As alterações sobem quando a conexão voltar. |
| **Sincronização** | Envio e recebimento automáticos, resolução de conflitos, lixeira, limites de banda e pausa. |
| **Backup** | Fazer backup de pastas do computador para o Drive e para o Google Fotos. |
| **Contas** | Várias contas Google ao mesmo tempo, além de Drives compartilhados e "Compartilhados comigo". |
| **Bandeja** | Ícone com atividade, notificações, erros, pesquisa e acesso rápido às preferências. |
| **Gerenciador de arquivos** | Ícones de status (na nuvem, off-line, sincronizando) e menu de botão direito: disponível off-line, compartilhar, copiar link, abrir no navegador. O KDE/Dolphin vem primeiro. |
| **Documentos Google** | Docs, Planilhas e Apresentações aparecem na pasta e abrem no navegador. |

## Como vai funcionar

```
┌──────────────────────────┐      ┌───────────────────────────────┐
│ Interface (Tauri)        │      │ Gerenciador de arquivos       │
│ bandeja, login,          │      │ (plugin do Dolphin: ícones de │
│ preferências, atividade  │      │  status e menu de contexto)   │
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

## Como experimentar a 0.1

A 0.1 é o motor, sem a bandeja: o login e o resto são feitos pela linha de comando provisória
`gdrive-desktop-linux`. É uma prévia para testes; mantenha cópia do que for importante.

**1. Compilar** (Rust 1.99 ou mais novo e FUSE 3):

```bash
sudo dnf install fuse3            # Fedora/Nobara (no Debian/Ubuntu: sudo apt install fuse3)
cargo build --release
```

**2. Instalar** os dois programas, o serviço do usuário e a abertura dos ponteiros `.gdoc`:

```bash
sudo install -Dm755 target/release/gdrive-desktop-linuxd target/release/gdrive-desktop-linux -t /usr/bin
sudo install -Dm644 packaging/systemd/gdrive-desktop-linuxd.service -t /usr/lib/systemd/user
sudo install -Dm644 packaging/dbus/io.github.junior9283.GDriveDesktopLinux1.service -t /usr/share/dbus-1/services
sudo install -Dm644 packaging/mime/gdrive-desktop-linux.xml -t /usr/share/mime/packages
sudo install -Dm644 packaging/desktop/io.github.junior9283.GDriveDesktopLinux.OpenPointer.desktop -t /usr/share/applications
sudo update-mime-database /usr/share/mime && sudo update-desktop-database
systemctl --user daemon-reload && systemctl --user enable --now gdrive-desktop-linuxd
```

**3. Entrar na conta** com o seu próprio Client ID (veja o [guia](docs/guia-client-id.md)):

```bash
gdrive-desktop-linux login        # pede o Client ID e o segredo e abre o navegador
gdrive-desktop-linux status       # conta, pasta, envios pendentes
```

A pasta aparece em `~/Google Drive/Meu Drive`. Outros comandos:

| Comando | O que faz |
|---|---|
| `pin <caminho>` / `unpin <caminho>` | deixa disponível off-line / só on-line |
| `pause` / `resume` | pausa e retoma a sincronização |
| `lost-and-found` | lista as edições guardadas por conflito |
| `logout [--finish \| --move <pasta> \| --discard]` | sai da conta, decidindo o que fazer com envios pendentes (o Lost and Found nunca é apagado) |
| `open-pointer <arquivo>` | abre um `.gdoc`, `.gsheet`… no navegador (usado pelo duplo clique) |

Limitações conhecidas da 0.1:

- **Vim e Neovim** (com o padrão `backupcopy=auto`) salvam renomeando o original e criando outro:
  o conteúdo não se perde, mas no Drive o arquivo ganha um ID novo e o antigo vai para a lixeira
  (links compartilhados do antigo deixam de valer). Com `set backupcopy=yes` o ID se mantém.
- Uma pasta criada aqui que ainda não subiu, mandada para a lixeira, some só deste computador.

Roteiro de teste da 0.1 (e o que já foi conferido): [docs/testes](docs/testes/README.md).

## Roteiro

Cada etapa sai funcionando antes da próxima.

| Versão | Etapa |
|---|---|
| 0.0.x | Design e especificação |
| 0.1 | Motor: pasta virtual, cache, sincronização e modo off-line ← **estamos aqui** |
| 0.2 | Bandeja, login e preferências |
| 0.3 | Integração com o gerenciador de arquivos (Dolphin) |
| 0.4 | Backup de pastas e Google Fotos |
| 0.5 | Várias contas e recursos extras |
| 1.0 | Paridade com a versão Windows |

## Versionamento

O projeto segue o [Versionamento Semântico](https://semver.org/lang/pt-BR/) (`MAIOR.MENOR.CORREÇÃO`).
Toda mudança é registrada no [CHANGELOG](CHANGELOG.md), e cada versão publicada ganha uma tag
`vX.Y.Z`.

## Como contribuir

Contribuições são bem-vindas, seja em código, testes, tradução, documentação ou relatos de uso.
Enquanto o projeto está na fase de design, a melhor forma de ajudar é abrir uma *issue* com
sugestões para a especificação.

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
