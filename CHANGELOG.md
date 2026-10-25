# Changelog

Todas as mudanças relevantes do projeto ficam registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e o projeto usa o
[Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado
- Documento de design com o catálogo de funcionalidades, a arquitetura, o roteiro e os testes
  (`docs/superpowers/specs/2026-10-25-gdrive-desktop-linux-design.md`).
- README: diferenças conhecidas em relação ao app oficial.
- Plano de implementação da versão 0.1 (motor) em `docs/superpowers/plans/`.
- Workspace Rust (8 crates), CI no GitHub Actions e mapeamento de nomes do Drive para nomes
  válidos no Linux (barra, `.`/`..`, limite de 255 bytes e nomes repetidos).

## [0.0.1] - 2026-10-25

### Adicionado
- Início do projeto: README com a explicação do app, o roteiro e as regras de contribuição.
- Licença GPL-3.0.
- `.gitignore` que impede a entrada de binários de terceiros e de material de engenharia reversa.
- Este CHANGELOG e a política de versionamento.

[Não lançado]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/junior9283/gdrive-desktop-linux/releases/tag/v0.0.1
