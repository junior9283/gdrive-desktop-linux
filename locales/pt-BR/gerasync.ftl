# Textos do GeraSync (`--cfg gerasync`) que trocam os do `main.ftl` e do `ui.ftl`: o nome do app
# e o login sem Client ID. O que fala do serviço do Google fica como está.

# ui.ftl
about-name = GeraSync
about-unofficial = O GeraSync é um cliente não oficial do Google Drive, sem ligação com o Google nem aprovação dele.
quit-title = Sair do GeraSync?
onboarding-files-text = O GeraSync é uma pasta em { $path }. Abra no Dolphin e use como qualquer outra.
onboarding-offline-command = gerasync pin <caminho>
onboarding-sign-in-text = Clique em Continuar: o navegador abre a página de login do Google.
onboarding-waiting-unverified = Se aparecer "O Google não verificou este app", clique em Avançado e depois em Acessar o GeraSync: a verificação do Google ainda não terminou. Seus dados só trafegam entre este computador e o Google.
pref-drive-folder = Pasta do GeraSync
onboarding-summary-mount = Pasta do GeraSync
folder-refused-nested = Esta pasta já está dentro de uma pasta sincronizada ou da pasta do GeraSync.
folder-refused-contains = Esta pasta contém uma pasta já sincronizada ou a pasta do GeraSync.
mirror-config-text = Uma cópia de todo o Meu Drive fica nesta pasta do computador, disponível mesmo sem internet. Os Drives compartilhados continuam na pasta do GeraSync.
uninstall-title = Desinstalar o GeraSync?
uninstall-body = Os programas saem deste computador e a pasta do GeraSync deixa de aparecer. Os arquivos no Google Drive, na nuvem, não são tocados.
uninstall-done = GeraSync desinstalado
share-missing = Abra esta janela pelo menu do GeraSync no gerenciador de arquivos.
versions-missing = Abra esta janela pelo menu do GeraSync no gerenciador de arquivos.

# main.ftl
login-page-title = GeraSync
notify-title = GeraSync
notify-offline-body = O GeraSync volta a sincronizar quando a internet voltar. O que está disponível off-line continua abrindo.
notify-disk-low-body = Restam { $free } MB livres. O GeraSync liberou o que pôde do cache.
cli-uninstall-confirm = Desinstalar o GeraSync deste computador? [s/N]
cli-uninstall-done = O GeraSync foi desinstalado.
cli-no-service = O serviço do GeraSync não está rodando ({ $error }).
tray-no-service = O serviço do GeraSync não está rodando
tray-open-folder = Abrir a pasta do GeraSync
tray-hotkey-search = Abrir a pesquisa do GeraSync
window-home = GeraSync
window-uninstall = Desinstalar o GeraSync
