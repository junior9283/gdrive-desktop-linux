# Guia: criar o seu Client ID do Google

Nas versões 0.x, o gdrive-desktop-linux não vem com uma identidade própria no Google. Cada pessoa cria
a sua, de graça, no Google Cloud. Leva uns 10 minutos e só precisa ser feito uma vez.

O primeiro uso do app traz este mesmo guia, com um botão que abre cada página do Google Cloud e
botões para copiar os escopos e os endereços da tela de consentimento.

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

1. Abra **Google Auth Platform → Branding** e clique em **Começar**.
2. Preencha o nome do app (por exemplo, `Drive no Linux`) e o seu e-mail de suporte.
3. Em **Público-alvo**, escolha **Externo**. Informe o e-mail de contato, aceite a política e clique em
   **Criar**.
4. De volta à página **Branding**:
   - **Não envie logotipo.** Com logotipo, o Google exige uma verificação demorada antes de publicar.
     Se já enviou, clique em **Remover**.
   - Em **Domínio do app**, preencha com as páginas do site do projeto (servem para qualquer pessoa):

     | Campo | Valor |
     |---|---|
     | Página inicial do aplicativo | `https://junior9283.github.io/gdrive-desktop-linux/` |
     | Link da Política de Privacidade | `https://junior9283.github.io/gdrive-desktop-linux/politica-de-privacidade.html` |
     | Link dos Termos de Serviço | `https://junior9283.github.io/gdrive-desktop-linux/termos-de-uso.html` |

   - Em **Domínios autorizados**, clique em **Adicionar domínio** e informe `junior9283.github.io`.
   - Clique em **Salvar** no fim da página.

## 3.1. Escolher as permissões (escopos)

1. Abra **Google Auth Platform → Acesso a dados** e clique em **Adicionar ou remover escopos**.
2. No fim do painel, em **Adicionar escopos manualmente**, cole os três, separados por vírgula, e
   clique em **Adicionar à tabela**:
   - `https://www.googleapis.com/auth/drive`
   - `openid`
   - `email`
3. Confira se os três ficaram marcados na tabela e clique em **Atualizar**.
4. De volta à página, clique em **Salvar** no fim dela. **Sem o Salvar, nada fica gravado.**
5. Ao final, `email` e `openid` aparecem em **Escopos não confidenciais** e o `drive` em
   **Seus escopos restritos**.

## 3.2. Publicar o app

1. Abra **Google Auth Platform → Público-alvo** e clique em **Publicar app**, depois em **Confirmar**.
2. Se aparecer *"Para publicar o app, conclua a configuração na página de branding"*, falta algum campo
   de **Domínio do app** ou há um logotipo enviado: volte ao item 4 do passo 3.
3. Depois de publicar, o Google mostra *"Seu aplicativo precisa ser verificado"*. Pode ignorar: sem a
   verificação, o app funciona para até 100 contas.

> **Importante:** não deixe o app "em teste". Nesse modo o Google invalida o login a cada 7 dias, e só
> entram as contas adicionadas em **Usuários de teste**. Em produção, sem verificação, o app continua
> privado: só quem tiver o seu Client ID consegue usá-lo.

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
| "Para publicar o app, conclua a configuração na página de branding" | Faltam a página inicial, a política ou o domínio autorizado, ou há um logotipo enviado. Corrija no item 4 do passo 3, salve e publique de novo. |
| `access_denied` ou "acesso recusado" | Você clicou em Cancelar, ou o app ainda está "em teste" e a sua conta não está em **Usuários de teste**. Publique o app (passo 3.2) ou adicione a sua conta em **Público-alvo → Usuários de teste**. |
| `redirect_uri_mismatch` | A credencial não é do tipo **App para computador**. Crie uma nova (passo 4). |
| O login para de funcionar depois de uma semana | O app ficou "em teste". Publique em produção (passo 3.2) e entre de novo. |
| "Google Drive API has not been used in project…" | A Drive API não foi ativada no projeto (passo 2). |
