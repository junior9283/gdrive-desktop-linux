# Política de Privacidade

O gdrive-desktop-linux é um cliente **não oficial** do Google Drive, de código aberto (GPL-3.0). Ele
não tem ligação com o Google nem é aprovado por ele.

## O que o app guarda no seu computador

- **Metadados dos arquivos** (nomes, pastas, tamanhos, datas e revisões) num banco local, para
  mostrar a pasta do Drive sem consultar a internet a cada acesso.
- **O conteúdo que você abriu ou marcou como off-line**, no cache, até ele ser liberado.
- **O token de acesso de cada conta conectada** (até 4), no Secret Service do seu ambiente (o "chaveiro" do KDE ou do
  GNOME), nunca em arquivo de texto.
- **Preferências e registros** do app, nas pastas de configuração e de estado do seu usuário.

## Com quem o app conversa

- Com as APIs do Google Drive e do login do Google, usando o Client ID que **você** criou no seu
  projeto do Google Cloud (veja o [guia do Client ID](guia-client-id.md)).
- Uma vez por dia, com a API pública do GitHub, só para saber qual é a última versão do app
  (`api.github.com/repos/junior9283/gdrive-desktop-linux/releases/latest`). O pedido leva apenas a
  versão instalada, no cabeçalho de identificação; nenhum dado seu nem dos seus arquivos.

O projeto não tem servidores próprios e não recebe nenhum dado seu.

## Para que o app usa os dados do Google

O app pede três permissões no login: o Google Drive (`drive`), para listar, abrir, enviar e
organizar os seus arquivos (e, quando você pede, compartilhá-los, ver as versões guardadas, criar atalhos ou criar um Drive compartilhado), e `openid` e `email`, para saber qual conta está conectada. Ele usa esses
dados só para mostrar e sincronizar o seu Drive no seu computador. Não os vende, não os usa para
publicidade e não os entrega a ninguém.

O uso e a transferência das informações recebidas das APIs do Google seguem a
[Política de dados do usuário dos serviços de API do Google](https://developers.google.com/terms/api-services-user-data-policy),
incluindo os requisitos de uso limitado.

## O que o app não faz

- Não envia estatísticas de uso nem telemetria.
- Nada é enviado sozinho. "Enviar feedback" e, com o relatório de erros ligado (vem
  **desligado**), "Enviar relatório" depois de uma falha só abrem no navegador a página de issues
  do projeto com o texto já preenchido: quem envia é você, pela sua conta do GitHub. Antes, o
  texto passa por um filtro que tira tokens, senhas e e-mails inteiros.
- O pacote de "Gerar informações de diagnóstico" fica no seu computador (passado pelo mesmo
  filtro); só sai dele se você anexá-lo.
- O servidor MCP (`gdrive-desktop-linux-mcp`) só atende o programa que o abriu no seu computador
  e não compartilha nada sem você confirmar na janela Compartilhar.
- A interface e o ícone na bandeja não acessam a internet: tudo passa pelo serviço local.

## Como apagar os dados

Desconecte a conta pela janela principal (menu da conta → Desconectar). O app remove o token, o
cache e os arquivos off-line daquela conta. As preferências ficam em
`~/.config/gdrive-desktop-linux` e podem ser apagadas à mão.

Os dados no Google Drive seguem as regras do próprio Google.
