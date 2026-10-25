# gdrive-desktop-linux

Cliente **não oficial** do Google Drive para Linux, escrito em Rust, com o mesmo comportamento,
aparência e funcionalidades do Google Drive para computador (Windows): pasta com arquivos sob
demanda, "Disponível off-line", backup de pastas, várias contas, Drives compartilhados, ícone na
bandeja e integração com o gerenciador de arquivos.

> *Unofficial Google Drive desktop client for Linux, written in Rust, aiming at feature and
> behavior parity with Google Drive for desktop on Windows. Contributions are welcome.*

## Situação

Em fase de **design**. A especificação ficará em [`docs/`](docs/) antes de qualquer código.

## Princípios

- **Código 100% próprio.** O app original foi estudado apenas para entender o comportamento
  (o que faz e como reage). Nenhum código, texto, ícone, fonte ou outro arquivo do Google é
  copiado para este repositório, e contribuições não devem incluir esse tipo de material.
- **Só APIs públicas.** O acesso usa a Google Drive API pública com OAuth; o app nunca se passa
  pelo cliente oficial do Google.
- **Nativo de Linux.** Pasta virtual via FUSE, motor em Rust, interface com a aparência do app
  original e integração com o ambiente de desktop (KDE primeiro).

## Aviso legal

Este projeto não tem vínculo com o Google LLC nem é endossado por ele. "Google Drive" é marca
registrada do Google LLC e é citada aqui apenas para indicar compatibilidade.

## Licença

[GNU GPL v3.0](LICENSE).
