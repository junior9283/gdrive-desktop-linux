# Deriva os arquivos do GeraSync dos da variante de sempre (scripts/install-dev.sh --flavor
# gerasync e o build.rs do gdl-daemon): os mesmos arquivos, com os nomes trocados. Vale para o
# conteúdo e para os caminhos.
#
# O endereço do repositório é o mesmo nas duas variantes: sai do caminho antes das trocas e volta
# no fim.
s|github\.com/junior9283/gdrive-desktop-linux|@@REPO@@|g
s/io\.github\.junior9283\.GDriveDesktopLinux/io.github.junior9283.GeraSync/g
s|/io/github/junior9283/GDriveDesktopLinux|/io/github/junior9283/GeraSync|g
s/gdrive-desktop-linux/gerasync/g
s/gdrive_desktop_linux/gerasync/g
s/gdl-emblem-/gerasync-emblem-/g
# O nome que o usuário vê (nos arquivos de packaging/ e integrations/, "Google Drive" é o app).
s/Google Drive para Linux/GeraSync/g
s/Google Drive/GeraSync/g
s|@@REPO@@|github.com/junior9283/gdrive-desktop-linux|g
