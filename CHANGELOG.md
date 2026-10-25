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

## [0.0.1] - 2026-10-25

### Adicionado
- Início do projeto: README com a explicação do app, o roteiro e as regras de contribuição.
- Licença GPL-3.0.
- `.gitignore` que impede a entrada de binários de terceiros e de material de engenharia reversa.
- Este CHANGELOG e a política de versionamento.

[Não lançado]: https://github.com/junior9283/gdrive-desktop-linux/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/junior9283/gdrive-desktop-linux/releases/tag/v0.0.1
