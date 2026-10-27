# Ajuda

## Primeiros passos

1. Crie o seu Client ID seguindo o [guia do Client ID](guia-client-id.md).
2. Clique no ícone do Google Drive na bandeja e escolha **Fazer login**.
3. Depois do login, a pasta `~/Google Drive` aparece com o `Meu Drive` dentro, e também no painel
   "Locais" do Dolphin. Os Drives compartilhados ficam em `~/Google Drive/Drives compartilhados`.

## Várias contas

Pelo menu da conta (a foto no alto da janela), **Adicionar outra conta** entra com mais uma conta
Google, até 4. Cada conta ganha a própria pasta, `~/Google Drive (<e-mail>)`, e todas sincronizam
ao mesmo tempo. Clicar numa conta da lista troca a conta mostrada na janela. Quando o acesso de uma
conta expira, ela aparece com **Entrar de novo**; as outras continuam sincronizando. Em **Mais
opções** há **Desconectar todas as contas**.

## Drives compartilhados

Em **Preferências › Google Drive › Gerenciar drives compartilhados** (ou pelo botão direito na
pasta "Drives compartilhados"), escolha quais drives aparecem na pasta e crie um novo. Mover um
arquivo para um Drive compartilhado muda quem pode vê-lo; mover uma pasta do Meu Drive para um
drive não é possível pelo app e precisa ser feito no Drive pela web.

## A janela principal

- **Cabeçalho:** o estado da sincronização, a última sincronização e o botão Pausar/Retomar.
- **Início:** o que precisa de atenção, os arquivos off-line, os compartilhados com você e o
  estado da sincronização.
- **Atividade de sincronização:** o que subiu, desceu ou foi para a lixeira, com o menu de cada
  linha (abrir, copiar link, cancelar um envio).
- **Notificações:** o histórico dos avisos.
- **Mais opções:** arquivos off-line, lista de erros, preferências, desconectar todas as contas e
  sair.

## Arquivos off-line

Use `gdrive-desktop-linux pin <caminho>` no terminal para deixar um arquivo ou pasta disponível
off-line, e `gdrive-desktop-linux unpin <caminho>` para voltar a deixá-lo só na nuvem. A tela
**Arquivos off-line** mostra o que está no computador e o espaço usado.

## Menu do botão direito

Nos arquivos e pastas do Drive, o gerenciador de arquivos ganha o submenu **Google Drive**: abrir no
navegador, copiar o link (o aviso diz quem pode abrir), disponível off-line ou só on-line,
**Compartilhar** (pessoas, papéis e acesso geral), **Gerenciar versões** (baixar, restaurar ou
manter para sempre), **Adicionar atalho ao Drive** (escolhendo a pasta) e, nos arquivos do Office,
abrir no Documentos, Planilhas ou Apresentações. No
Dolphin, cada arquivo mostra também um emblema: na nuvem, off-line, sincronizando ou em conflito.

No KDE, o app tira a pasta do Drive do indexador de arquivos (Baloo), que baixaria tudo para ler o
conteúdo; as outras pastas excluídas continuam como estavam. Desinstalar desfaz a mudança.

## Algo deu errado

Veja as [perguntas frequentes](perguntas-frequentes.md). Se o problema continuar, abra um relato em
<https://github.com/junior9283/gdrive-desktop-linux/issues>.
