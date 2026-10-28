# Ajuda

## Primeiros passos

1. Crie o seu Client ID seguindo o [guia do Client ID](guia-client-id.md).
2. Clique no ícone do Google Drive na bandeja e escolha **Fazer login**.
3. Depois do login, a pasta `~/Google Drive` aparece com o `Meu Drive` dentro, e também no painel
   "Locais" do Dolphin. Os Drives compartilhados ficam em `~/Google Drive/Drives compartilhados`.
4. O primeiro uso termina com um **Resumo** do que foi escolhido e oferece o **tour** da janela
   principal, que também fica nas Preferências.

## Várias contas

Pelo menu da conta (a foto no alto da janela), **Adicionar outra conta** entra com mais uma conta
Google, até 4. Cada conta ganha a própria pasta, `~/Google Drive (<e-mail>)`, e todas sincronizam
ao mesmo tempo. Clicar numa conta da lista troca a conta mostrada na janela. Quando o acesso de uma
conta expira, ela aparece com **Entrar de novo**; as outras continuam sincronizando. Em **Mais
opções** há **Desconectar todas as contas**.

## Drives compartilhados

Em **Preferências › Google Drive › Gerenciar drives compartilhados** (ou pelo botão direito na
pasta "Drives compartilhados", ou na visão **Drives compartilhados** da janela principal), escolha quais drives aparecem na pasta e crie um novo. Mover um
arquivo para um Drive compartilhado muda quem pode vê-lo; mover uma pasta do Meu Drive para um
drive não é possível pelo app e precisa ser feito no Drive pela web. O app avisa quando um item
vai para um Drive compartilhado ou para a pasta de outra pessoa, e quando um Drive compartilhado
tem muita atividade.

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
  linha (abrir, copiar link, compartilhar, cancelar um envio). Com os envios pausados, a fila
  aparece como "Pausado".
- **Drives compartilhados:** abrir cada drive na pasta ou na web.
- **Notificações:** o histórico dos avisos.
- **Mais opções:** arquivos off-line, lista de erros, preferências, Drives compartilhados,
  desconectar todas as contas e sair.
- **Preferências:** as abas **Meu computador** e **Google Drive**; as demais configurações ficam
  atrás da engrenagem.

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

## Atividade suspeita nos arquivos

Se muitos arquivos forem regravados de um jeito que lembra um vírus que cifra arquivos (vinte em um
minuto), o app pausa os envios da conta e avisa. As versões anteriores continuam no Drive. No
aviso: **Verificar** abre a janela principal, **Eu reconheço esta atividade** retoma e não avisa de
novo por uma hora (por exemplo, quando você mesmo está cifrando arquivos) e **Permitir
sincronização** retoma. Fotos, vídeos, `.zip` e outros formatos já comprimidos não contam.

## Itens recuperados

Se os dados do app de uma conta se danificarem (queda de energia, disco com defeito), o app os
guarda numa pasta `recuperacao` e lê a conta de novo do Drive. As edições que ainda não tinham
subido vão para o Lost and Found, na pasta "Recuperados <data>"; o aviso "Itens recuperados" leva
até elas. Nada é reenviado sozinho: mova de volta para a pasta do Drive o que quiser manter. No
aviso do Lost and Found, **Mover tudo para…** leva tudo para uma pasta escolhida, sem sobrescrever
nada.

## Assistentes de IA (MCP)

O programa `gdrive-desktop-linux-mcp` deixa um assistente compatível com o Model Context Protocol
consultar o estado e o link de um caminho, ver as pastas acompanhadas, abrir a janela Compartilhar
com um e-mail já digitado e adicionar uma pasta à sincronização. Ele só aceita pastas visíveis
dentro da sua pasta pessoal e nunca compartilha nada sem você confirmar. O README mostra como
registrar o servidor no seu cliente.

## Algo deu errado

Em **Mais opções**:

- **Enviar feedback** abre no navegador uma issue do projeto já preenchida com a versão e o
  sistema; marque "Incluir informações de diagnóstico" para gerar o pacote e anexá-lo.
- **Gerar informações de diagnóstico** grava na Área de trabalho um `.tar.gz` com os registros, o
  sistema e as preferências, sem tokens, senhas nem e-mails inteiros.
- **Ativar o registro detalhado** grava mais detalhes nos registros por um dia (ou até desativar).
- **Redefinir os dados locais** apaga os dados do app da conta neste computador e a lê de novo do
  Drive, com o mesmo login (só sem envios pendentes).

Com "Enviar relatórios de erro automaticamente" ligado nas Preferências, depois de uma falha o app oferece abrir a
issue com o relatório; quem envia é você.

Veja também as [perguntas frequentes](perguntas-frequentes.md) e as issues em
<https://github.com/junior9283/gdrive-desktop-linux/issues>.
