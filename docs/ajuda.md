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

## Espelho do Meu Drive

Em **Preferências › Google Drive**, **Espelhar arquivos** leva o Meu Drive inteiro para uma pasta
comum do computador (sugerida ao lado da pasta do Google Drive; dá para escolher outra), disponível
mesmo sem internet. Antes de confirmar, a janela mostra quanto será baixado e o espaço livre. Os
Drives compartilhados continuam na pasta do Google Drive. **Fazer streaming dos arquivos** volta ao
modo anterior depois que tudo o que foi alterado subir, e pode mandar a pasta do Espelho para a
lixeira do sistema.

## Pastas do computador

Em **Preferências › Meu computador**, **Adicionar pasta** sincroniza uma pasta do computador com
"Computadores/<este computador>" no Drive, nos dois sentidos. No Dolphin, o botão direito numa pasta
fora do Drive oferece **Sincronizar esta pasta com o Google Drive**. Nada é apagado de vez: o que
você exclui no computador vai para a lixeira do Drive, e o que sai do Drive vai para a lixeira do
sistema. Se muitos arquivos somem de uma vez, a pasta para e pergunta antes. Pastas de rede não
podem ser sincronizadas.

## Pendrives e cartões de memória

Quando um pendrive, cartão ou câmera é conectado, o app pergunta se quer fazer backup. O backup só
envia (nada é apagado do dispositivo nem do Drive) e vai para "Computadores/<este computador>/<nome
do dispositivo>"; no fim, o aviso diz que dá para remover o dispositivo com segurança. Em
**Preferências › Meu computador** dá para fazer backup sempre que o dispositivo for conectado,
ignorá-lo ou desligar a pergunta.

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
