# Perguntas frequentes

**O app é do Google?**
Não. É um cliente não oficial e de código aberto, que usa as APIs públicas do Google Drive.

**Por que preciso criar um Client ID?**
O app não tem servidores nem credenciais próprias. Você cria o seu Client ID no Google Cloud e só
você usa. O passo a passo está no [guia do Client ID](guia-client-id.md).

**Os arquivos ocupam espaço no meu disco?**
No modo Stream, só o que você abre ou marca como "Disponível off-line" fica no disco. O resto fica
na nuvem e aparece na pasta do Drive como se estivesse no computador.

**Posso usar sem internet?**
Sim, com os arquivos que estão off-line ou já em cache. O que você alterar sem internet sobe quando
a conexão voltar.

**Apaguei um arquivo sem querer. E agora?**
Os arquivos apagados vão para a lixeira do Drive (a pasta oculta `.Lixeira do Drive` dentro da
pasta do Drive) e podem ser restaurados de lá ou pela web.

**Onde ficam os registros para relatar um problema?**
Rode `journalctl --user -u gdrive-desktop-linuxd` para ver os registros do serviço. Abra um relato em <https://github.com/junior9283/gdrive-desktop-linux/issues>.
