# Checklist do Console: verificação do GeraSync

Passos no Google Cloud Console e no Search Console, na ordem, para o GeraSync sair com o Client ID
oficial e passar pela verificação da marca e do escopo restrito `drive`. Os cliques de envio no
Google e a gravação do vídeo são seus; os textos em inglês para colar estão nesta pasta.

Valores usados em todo o pedido:

| Campo | Valor |
|---|---|
| Nome do app | `GeraSync for Google Drive` |
| E-mail de suporte e de contato | `gerasync@googlegroups.com` |
| Logotipo (120 × 120) | `assets/gerasync/logo-120.png` |
| Página inicial | `https://gerasync.solusapp.net/` |
| Política de privacidade | `https://gerasync.solusapp.net/privacy.html` |
| Termos de serviço | `https://gerasync.solusapp.net/terms.html` |
| Domínio autorizado | `solusapp.net` |
| Escopos | `openid`, `.../auth/userinfo.email`, `https://www.googleapis.com/auth/drive` |
| Tipo do cliente | App para computador |
| Segredos do repositório | `GERASYNC_CLIENT_ID`, `GERASYNC_CLIENT_SECRET` |

## 1. Projeto de produção

- [ ] Criar um projeto novo só para o GeraSync (ex.: `gerasync`), separado do projeto de testes.
  O pedido de verificação vale para o projeto inteiro.

## 2. Drive API

- [ ] APIs e serviços → Biblioteca → **Google Drive API** → Ativar.

## 3. Branding (Google Auth Platform → Branding)

- [ ] Nome do app, e-mail de suporte e logotipo da tabela acima.
- [ ] Página inicial, política de privacidade e termos da tabela acima.
- [ ] Domínio autorizado: `solusapp.net`.
- [ ] Contato do desenvolvedor: `gerasync@googlegroups.com`.

## 4. Domínio e Search Console

O domínio autorizado precisa estar confirmado no Search Console pela mesma conta dona do projeto.
O site fica no subdomínio `gerasync.solusapp.net` (o motivo está em `historico.md`).

- [ ] Cloudflare, zona `solusapp.net`: TXT no `@` com o `google-site-verification=...` que o
  Search Console mostra para a propriedade **Domínio** `solusapp.net`.
- [ ] Cloudflare: CNAME `gerasync` → `junior9283.github.io`, **sem proxy** (nuvem cinza), para o
  GitHub emitir o certificado.
- [ ] Repositório `junior9283/gerasync`: arquivo `CNAME` com `gerasync.solusapp.net` e "Enforce
  HTTPS" ligado nas configurações do Pages.
- [ ] Search Console → propriedade **Domínio** `solusapp.net` → **Verificar**.

## 5. Acesso a dados (Google Auth Platform → Acesso a dados)

- [ ] Adicionar os escopos `openid`, `.../auth/userinfo.email` e
  `https://www.googleapis.com/auth/drive`.
- [ ] Recursos: **Drive sync client** e **Drive backup**.
- [ ] Justificativa do `drive`: colar a versão curta de `scope-justification.md` (o campo aceita
  1000 caracteres).
- [ ] Link do vídeo: o vídeo não listado do YouTube gravado com `video-script.md`.

## 6. Cliente OAuth (Google Auth Platform → Clientes)

- [ ] Criar cliente → Tipo **App para computador** → nome `GeraSync desktop`.
- [ ] **Não cole o ID nem o segredo no chat.** No terminal, na pasta do repositório, rode cada
  comando e cole o valor quando ele pedir:

  ```sh
  # cole o "ID do cliente" (termina em .apps.googleusercontent.com)
  gh secret set GERASYNC_CLIENT_ID -R junior9283/gdrive-desktop-linux
  # cole a "Chave secreta do cliente" (começa com GOCSPX-)
  gh secret set GERASYNC_CLIENT_SECRET -R junior9283/gdrive-desktop-linux
  ```

  Na 1.3.0 os dois foram cadastrados trocados e o login do GeraSync dava `invalid_client`; o
  release agora recusa os segredos trocados antes de compilar.

- [ ] Conferir com `gh secret list -R junior9283/gdrive-desktop-linux` (mostra só os nomes).

## 7. Publicar

- [ ] Google Auth Platform → Público → **Publicar app** (status "Em produção"). Até a verificação
  terminar, quem entra vê o aviso de app não verificado e vale o teto de 100 contas.

## 8. Verificação da marca

- [ ] Central de verificação → enviar a verificação da marca (nome, logotipo, domínio). Responder
  pelo e-mail de contato o que o Google perguntar; costuma sair em poucos dias.
- [ ] Esperar 24 h depois de verificar a propriedade no Search Console: antes disso a checagem
  automática recusa a página inicial como "not registered to you". O andamento fica em
  `historico.md`.

## 9. Verificação do escopo restrito

- [ ] Gerar um release com os segredos (tag `v*`), instalar o pacote `gerasync` e gravar o vídeo.
- [ ] Central de verificação → enviar a verificação do escopo, com a justificativa e o vídeo.
- [ ] Se perguntarem da avaliação de segurança (CASA), responder com `casa-answer.md`. A decisão é
  do Google; se ele exigir a avaliação, há custo e prazo extras.

## Depois de aprovado

- [ ] **Reverificação anual**: o Google pede de novo a cada 12 meses para apps com escopo restrito.
  Anotar a data da aprovação e conferir o e-mail do grupo.
- [ ] Manter a página inicial, a política e os termos no ar e com o mesmo nome do app.
- [ ] Qualquer escopo novo exige outro pedido de verificação antes de entrar num release.
