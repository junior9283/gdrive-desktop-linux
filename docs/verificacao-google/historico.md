# Histórico do pedido de verificação do GeraSync

O que já foi feito no Google, com data, para retomar sem refazer. Os passos estão em
`checklist.md`.

## 2026-11-04

- Search Console: propriedade `https://junior9283.github.io/` (prefixo do URL) verificada pela
  tag HTML, na conta dona do projeto `gerasync`.
- Segredos do Client ID corrigidos (estavam trocados na 1.3.0); release 1.3.1 com login funcionando.
- Google Auth Platform → Público: app publicado (**In production**). Logo depois de publicar, o
  consentimento deu erro 500 algumas vezes; passou sozinho em poucos minutos.

## 2026-11-05

- Vídeo de demonstração gravado com `video-script.md` (5 min 46 s, conta de teste) e enviado ao
  YouTube como **Não listado**: https://youtu.be/d-jxEqwkXvQ
- Data Access salvo: recursos **Drive sync client** e **Drive backup**, a versão curta da
  justificativa (`scope-justification.md`, o campo aceita 1000 caracteres) e o link do vídeo.
- "Verify branding" rodou a checagem automática e recusou:
  *The website of your home page URL "https://junior9283.github.io/gerasync/" is not registered to
  you.* O Google pede para esperar 24 h depois de verificar a propriedade antes de tentar de novo;
  a propriedade foi adicionada em 04/11.
- A verificação do escopo (Verification Center → Data access) só abre depois da marca verificada.

## 2026-11-06

- "I have fixed the issues" → Proceed: recusado de novo com a mesma mensagem, mesmo com a
  propriedade verificada havia mais de 24 h pela conta dona do projeto. O Google não aceita o
  `github.io` (o domínio é do GitHub).
- Decisão: site no subdomínio `gerasync.solusapp.net`, do domínio `solusapp.net` (Cloudflare), que
  já é do usuário.
- Cloudflare (`solusapp.net`): TXT de verificação do Google no `@` e CNAME `gerasync` →
  `junior9283.github.io` sem proxy. Search Console: propriedade **Domínio** `solusapp.net`
  verificada ("Ownership auto verified").
- GitHub Pages do repositório `gerasync`: domínio `gerasync.solusapp.net` com HTTPS obrigatório; o
  endereço antigo redireciona (301).
- Branding salvo com a página inicial, a privacidade e os termos em `https://gerasync.solusapp.net/`
  e o domínio autorizado `solusapp.net`.

## Próximo passo

- A partir de 2026-11-07: Branding → View issues → **I have fixed the issues** → Proceed (24 h
  depois de verificar a propriedade `solusapp.net`).
- Com a marca verificada: Verification Center → **Prepare for verification** do escopo `drive`.

## Observações

- "Disconnect account" no GeraSync não revoga a permissão no Google; para regravar o consentimento
  completo é preciso remover o acesso em myaccount.google.com → Segurança → Apps de terceiros.
