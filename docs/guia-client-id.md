# Guia: criar o seu Client ID do Google

Nas versões 0.x, o gdrive-desktop-linux não vem com uma identidade própria no Google. Cada pessoa cria
a sua, de graça, no Google Cloud. Leva uns 10 minutos e só precisa ser feito uma vez.

O primeiro uso do app traz este mesmo guia, com um botão que abre cada página do Google Cloud e
botões para copiar os escopos.

O Client ID e o segredo ficam guardados no chaveiro do sistema (KWallet ou GNOME Keyring), junto com o
login, e nunca em arquivo.

## 1. Criar um projeto

1. Abra o [Google Cloud Console](https://console.cloud.google.com/) com a conta Google que você vai usar.
2. No seletor de projetos, no alto da página, clique em **Novo projeto**.
3. Dê um nome (por exemplo, `Drive no Linux`) e clique em **Criar**.
4. Confira se o projeto novo está selecionado antes de seguir.

## 2. Ativar a Google Drive API

1. No menu, abra **APIs e serviços → Biblioteca**.
2. Procure por **Google Drive API**, abra e clique em **Ativar**.

## 3. Configurar a tela de consentimento

1. Abra **APIs e serviços → Tela de consentimento OAuth** (ou **Google Auth Platform → Branding**).
2. Escolha o tipo de usuário **Externo** e preencha o nome do app e o seu e-mail de contato.
3. Em **Escopos** (ou **Acesso a dados**), adicione:
   - `https://www.googleapis.com/auth/drive`
   - `openid`
   - `email`
4. Em **Público-alvo**, clique em **Publicar app** para colocá-lo **em produção**.

> **Importante:** não deixe o app "em teste". Nesse modo o Google invalida o login a cada 7 dias e você
> teria que entrar de novo toda semana. Em produção, sem verificação, o app continua privado: só quem
> tiver o seu Client ID consegue usá-lo.

## 4. Criar a credencial

1. Abra **APIs e serviços → Credenciais**.
2. Clique em **Criar credenciais → ID do cliente OAuth**.
3. Em **Tipo de aplicativo**, escolha **App para computador**.
4. Dê um nome e clique em **Criar**.
5. Copie o **ID do cliente** e a **Chave secreta do cliente**. Você vai colá-los no primeiro login.

## 5. Primeiro login

1. No primeiro uso do app, passe do guia (Continuar) e cole o ID e a chave secreta.
2. O navegador abre a página de login do Google. Escolha a conta.
3. Como o app não passou pela verificação do Google, aparece o aviso **"O Google não verificou este app"**.
   Clique em **Avançado → Acessar (nome do app) (não seguro)**. O aviso existe porque o app é seu e não
   foi enviado ao Google para revisão; os dados só trafegam entre o seu computador e o Google.
4. Autorize o acesso ao Google Drive. A página mostra "Pronto! Pode fechar esta janela e voltar ao app."

## Problemas comuns

| Mensagem | O que fazer |
|---|---|
| `access_denied` ou "acesso recusado" | Você clicou em Cancelar, ou o app ainda está "em teste" e a sua conta não está na lista de testadores. Publique o app em produção (passo 3.4). |
| `redirect_uri_mismatch` | A credencial não é do tipo **App para computador**. Crie uma nova (passo 4). |
| O login para de funcionar depois de uma semana | O app ficou "em teste". Publique em produção e entre de novo. |
| "Google Drive API has not been used in project…" | A Drive API não foi ativada no projeto (passo 2). |
