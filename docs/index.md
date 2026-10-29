---
title: gdrive-desktop-linux
---

<p align="center">
  <img src="https://raw.githubusercontent.com/junior9283/gdrive-desktop-linux/main/assets/icons/hicolor/128x128/apps/io.github.junior9283.GDriveDesktopLinux.png" alt="Ícone do gdrive-desktop-linux" width="128">
</p>

# gdrive-desktop-linux

Cliente **não oficial** do Google Drive para Linux, de código aberto (GPL-3.0), escrito em Rust.
Ele mostra o seu Drive como uma pasta no computador, em que os arquivos ficam na nuvem até você
abrir, com ícone na bandeja, pesquisa, arquivos disponíveis off-line, sincronização automática,
até 4 contas ao mesmo tempo, Drives compartilhados, o modo Espelho (o Meu Drive inteiro numa pasta
do computador), pastas do computador sincronizadas, backup de pendrives, um tour de boas-vindas e, no gerenciador de
arquivos, emblemas de estado (inclusive de erro), compartilhamento, versões e atalhos pelo botão direito. Desde a 1.0
tem paridade com o app oficial para Windows, item a item na [matriz de paridade](paridade.md); a 1.1
fecha as pendências dela e a 1.2 tira as esperas sem fim da desmontagem, da troca de pasta e dos avisos. Ele também
pausa os envios diante de arquivos que parecem cifrados por um vírus, recupera as edições se os
dados locais se danificarem e tem um servidor MCP para assistentes de IA.

O app roda inteiro no seu computador e conversa com as APIs do Google, usando um Client ID que
você mesmo cria no seu projeto do Google Cloud (e, uma vez por dia, pergunta ao GitHub qual é a
última versão do app). O projeto não tem servidores e não recebe nenhum
dado seu. Ele não tem ligação com o Google nem é aprovado por ele.

- [Código-fonte e instalação](https://github.com/junior9283/gdrive-desktop-linux)
- [Política de Privacidade](politica-de-privacidade.md)
- [Termos de uso](termos-de-uso.md)
- [Guia do Client ID](guia-client-id.md)
- [Ajuda](ajuda.md) · [Perguntas frequentes](perguntas-frequentes.md)

> **English:** gdrive-desktop-linux is an unofficial, open-source (GPL-3.0) Google Drive desktop
> client for Linux. It runs entirely on your computer and talks to Google's APIs, with an OAuth
> client you create in your own Google Cloud project (and, once a day, asks GitHub for the latest
> app version). The project has no servers and receives none
> of your data. See the [Privacy Policy](politica-de-privacidade.md) and the
> [Terms of use](termos-de-uso.md).
