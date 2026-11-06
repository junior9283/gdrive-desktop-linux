# Deriva os arquivos do GeraSync dos da variante de sempre (scripts/install-dev.sh --flavor
# gerasync, scripts/package.sh, integrations/dolphin/build.sh e o build.rs do gdl-daemon): os
# mesmos arquivos, com os nomes trocados. Vale para o conteúdo e para os caminhos.
#
# O que fala do serviço e do repositório, que são os mesmos nas duas variantes, sai do caminho
# antes das trocas (@@REPO@@, @@DRIVE@@) e volta no fim.
s|github\.com/junior9283/gdrive-desktop-linux|@@REPO@@|g
# Os <comment> dos tipos dos ponteiros ("Link do Google Drive") e a descrição do pacote (nfpm).
/<comment/s/Google Drive/@@DRIVE@@/g
s/^  Cliente não oficial do Google Drive para Linux\.$/  GeraSync: cliente não oficial do @@DRIVE@@ para Linux./
# A página do pacote (nfpm) é o site do GeraSync.
s|^homepage: .*|homepage: https://gerasync.solusapp.net/|
# O ID do app (D-Bus, .desktop, ícone) e os nomes de classe dos menus.
s/GDriveDesktopLinux/GeraSync/g
s/gdrive-desktop-linux/gerasync/g
s/gdrive_desktop_linux/gerasync/g
# O ID do plugin de emblemas do Dolphin e os emblemas, que ficam no tema do sistema.
s/gdrivedesktoplinux/gerasync/g
s/gdl-emblem-/gerasync-emblem-/g
# O nome que o usuário vê: no resto dos arquivos de packaging/ e integrations/, "Google Drive" é
# o app.
s/Google Drive para Linux/GeraSync/g
s/Google Drive/GeraSync/g
# O install.sh do .tar.gz instala a variante dele.
s/--flavor default/--flavor gerasync/g
s/@@DRIVE@@/Google Drive/g
s|@@REPO@@|github.com/junior9283/gdrive-desktop-linux|g
