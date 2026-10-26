# Miniaturas sem baixar os arquivos — projeto

Data: 2026-10-26. Versão alvo: 0.3.0 (funcionalidade nova).

## Intenção

Hoje, ao abrir no Dolphin uma pasta do Drive com fotos, o KDE gera as prévias lendo cada arquivo
inteiro pela montagem: a pasta "baixa tudo" e o cache cresce (na primeira sessão real, 131 MB só de
prévias). O "sob demanda" deixa de valer justamente nas pastas de fotos.

O usuário quer que as miniaturas apareçam **sem baixar os arquivos**, como ele espera do Drive para
computador no Windows. Na prática (testado no Windows, Drive 132): o app do Google busca a
miniatura pronta do servidor só para .png, .bmp e tipos do Google; para .jpg o Explorer baixa o
arquivo inteiro. Aqui a meta é **melhor que o Windows**: nenhum tipo com miniatura no servidor
deve ser baixado só para gerar a prévia.

Escolha do usuário (opção 2): o serviço busca as miniaturas que o Google já gera
(`thumbnailLink`) e as grava no **cache de miniaturas do freedesktop**, onde o Dolphin (e o
Nautilus, o Nemo etc.) procuram antes de gerar uma prévia. O usuário aceitou uma espera de **até
1–2 s na primeira abertura** de cada pasta.

### O que o protótipo provou (descartado, 2026-10-26)

- Com PNGs gravados em `~/.cache/thumbnails/{normal,large,…}/MD5(uri).png` contendo
  `Thumb::URI` e `Thumb::MTime` iguais aos da montagem, o Dolphin mostrou as prévias e leu
  **0 bytes** dos 8 .jpg semeados; os 9 .heic não semeados foram baixados inteiros (~23 MB).
- A URI que funcionou: `file://` + o caminho com percent-encoding mantendo
  `/!$&'()*+,;=:@~` sem codificar (o mesmo que `QUrl::toEncoded`/GLib para caminhos).
- O KIO só consulta o cache para tipos que têm um gerador de miniaturas registrado (imagens,
  vídeos, PDF…). Os atalhos de documentos do Google (`.gdoc` etc.) não têm, então nada é buscado
  para eles.

## Objetivos

1. Ao abrir (listar) uma pasta da montagem pela primeira vez, o serviço grava no cache do
   freedesktop as miniaturas do Google dos arquivos dessa pasta que tenham miniatura no servidor,
   **antes** de responder a listagem, esperando no máximo ~1,5 s; o que não terminar nesse prazo
   continua em segundo plano.
2. Abrir de novo a mesma pasta não espera nada nem refaz downloads.
3. Quando um arquivo muda (nova revisão ou nova data de modificação), a miniatura dele é refeita
   na próxima abertura da pasta.
4. Serve para qualquer gerenciador que siga a especificação de miniaturas do freedesktop, não só
   o Dolphin.

## Fora do escopo

- Miniaturas dos documentos do Google (Docs, Planilhas, Apresentações): exigiriam um plugin de
  miniaturas do KDE em C++ (fica para a versão do plugin do Dolphin, já prevista no projeto
  principal, §6.12).
- Arquivos sem miniatura no servidor (o Google não gera para tudo, ex.: alguns .heic/.raw): o
  gerenciador continua gerando a prévia pelo arquivo, como hoje.
- Excluir a montagem do Baloo (outra tarefa do §6.12).
- Pré-carregar pastas que o usuário não abriu.

## Anti-objetivos (falhas mesmo que "funcione")

- Travar a montagem: nenhuma outra operação do FUSE pode ficar esperando a busca de miniaturas
  (a espera é só da listagem daquela pasta, numa das threads do FUSE).
- Listagem lenta sempre: a espera acontece só na primeira abertura (ou após mudança); pastas sem
  imagens não esperam nada.
- Prévia errada: uma miniatura velha mostrada para um arquivo que mudou.
- Gastar cota/rede à toa: baixar miniaturas de pastas inteiras recursivamente ou de arquivos que
  já estão completos no cache local (para esses o próprio gerenciador gera a prévia sem rede).
- Vazar dados: miniaturas fora do diretório de miniaturas do usuário, permissões abertas, ou o
  token de acesso enviado a servidores que não sejam do Google.

## Restrições

- Rust, no motor/serviço existente; nada de código, textos ou ícones do Google.
- O `thumbnailLink` vem da API do Drive v3 (campo `thumbnailLink`, `hasThumbnail`); o tamanho é
  pedido trocando o sufixo `=sNNN` do link.
- Os arquivos gravados seguem a especificação de miniaturas do freedesktop (0.8.x):
  PNG RGBA, lado maior = 128/256/512/1024 conforme a pasta (`normal`, `large`, `x-large`,
  `xx-large`), texto `Thumb::URI` e `Thumb::MTime` obrigatórios (`Thumb::Size` recomendado),
  permissões 0600, diretórios 0700, gravação atômica (arquivo temporário + rename no mesmo
  diretório).
- `Thumb::MTime` precisa ser igual ao `st_mtime` que a montagem informa para o arquivo
  (hoje `modified_ms / 1000`, em `gdl-fs/src/fs.rs`, `time_of`).
- `XDG_CACHE_HOME` respeitado (padrão `~/.cache`).
- Testes: Drive simulado (`gdl-fake-drive`) passa a servir `thumbnailLink` e as imagens; nenhum
  teste usa a rede nem a conta real.

## Como funciona (decidido)

1. **Gatilho:** `opendir` de uma pasta da montagem. Se a pasta ainda não foi semeada (ou algum
   arquivo dela mudou desde a última semeadura), o motor dispara a semeadura dessa pasta e o
   `opendir` espera até ~1,5 s por ela antes de responder. Passado o prazo, responde e a
   semeadura segue em segundo plano.
2. **Quem semear:** os arquivos diretos da pasta (não recursivo) que não são do Google, não estão
   completos no cache local e têm miniatura no servidor.
3. **Busca:** uma chamada de listagem da pasta pedindo `id, thumbnailLink, hasThumbnail,
   modifiedTime` (o `thumbnailLink` expira em poucas horas, então não é guardado no banco), e
   então os downloads das imagens em paralelo, com limite de concorrência.
4. **Gravação:** para cada arquivo, um PNG por tamanho em `$XDG_CACHE_HOME/thumbnails/<tamanho>/
   MD5(uri).png`, com a URI do caminho do arquivo **na montagem atual**.
5. **Memória:** o motor guarda, por arquivo semeado, a versão semeada (revisão/data de
   modificação) e o caminho; pasta já semeada com tudo em dia não dispara nada.
6. **Ao sair da conta:** apaga as miniaturas que o app gravou (a lista vem dessa memória).

## Decisões do autor do documento (não confirmadas pelo usuário)

Tomadas depois da leitura de um revisor; o usuário pode mudar qualquer uma.

- **Divisão entre crates:** a semeadura mora no motor (`gdl-engine`, módulo novo), que recebe o
  caminho absoluto de cada arquivo. O ponto de montagem é passado pelo daemon ao `gdl-fs` na
  montagem; o `opendir` monta os caminhos e chama o motor pelo runtime que já existe (`rt`),
  esperando com prazo.
- **Quem fica de fora:** arquivos com alteração local ou envio pendente, em conflito, ou já
  completos no cache local. Arquivos parcialmente em cache são semeados (a miniatura do servidor
  bate com a versão da nuvem, que é a que a montagem mostra).
- **`Thumb::MTime`:** o mesmo valor que o `getattr` da montagem devolve; a função que converte
  o tempo passa a ser única para os dois usos.
- **Detecção de mudança:** só pelo banco local, que o feed de mudanças já mantém atualizado. A
  memória guarda, por arquivo, `headRevisionId` + `modified_ms` + a URI gravada. A chamada de
  listagem ao Google só acontece se houver ao menos um arquivo a semear; pasta em dia não usa a
  rede.
- **Tamanhos:** um download por arquivo pedindo `=s512`, reduzido localmente para `normal` (128),
  `large` (256) e `x-large` (512). `xx-large` (1024) fica de fora (o Google não garante essa
  resolução e ampliar não ajuda); nesse zoom o gerenciador gera pelo arquivo, como hoje.
- **Não sobrescrever:** só grava se o PNG não existe ou se o `Thumb::MTime` dele é diferente
  (uma miniatura que o próprio gerenciador gerou para a mesma versão é mantida).
- **Pastas grandes:** sem teto de arquivos; a listagem é paginada, os downloads em paralelo com
  limite de 8, e a semeadura de fundo é cancelada ao sair da conta ou desmontar. Miniatura é
  pequena (dezenas de KB), então o custo é aceitável.
- **Token de acesso:** se o `thumbnailLink` exigir o token, ele só vai a `https://` em hosts
  `*.googleusercontent.com` / `*.google.com` ou à mesma origem da API (o Drive simulado dos
  testes) — a regra de `fetch_photo` em `gdl-drive/src/client.rs`. O Drive simulado serve
  `thumbnailLink` na própria origem, e um teste prova que o token não vai a outro host.
- **Limpeza ao sair da conta:** apaga os PNGs listados na memória (URI → MD5 → arquivo, nos três
  tamanhos). Miniaturas órfãs por queda no meio da gravação, troca do ponto de montagem ou
  renomeação ficam no cache como lixo inofensivo (apontam para caminhos que não existem mais);
  sem varrer o cache inteiro.
- **Renomear/mover:** a URI muda, então o arquivo é semeado de novo na próxima abertura da pasta
  nova.

## Decisões deixadas ao construtor

- A biblioteca de imagem/PNG.
- Onde a memória fica (tabela nova no `metadata.db` ou na `kv`), respeitando a migração do banco.
- O prazo exato (entre 1 e 2 s).
