# Pastas virtuais da montagem
folder-my-drive = Meu Drive
folder-shared-drives = Drives compartilhados
folder-trash = .Lixeira do Drive
folder-lost-found = .Lost and Found
folder-canceled-uploads = .Uploads cancelados
folder-shortcut-targets = .shortcut-targets-by-id

# Montagem
conflict-word = conflito
computers-folder = Computadores
mount-moved = A pasta do Drive foi aberta em { $path }

# Página mostrada no navegador ao fim do login
login-page-title = Google Drive para Linux
login-success = Pronto! Pode fechar esta janela e voltar ao app.
login-failure = Não foi possível entrar na conta. Feche esta janela e tente de novo no app.

# Notificações
notify-title = Google Drive
notify-account = Conta: { $email }
notify-lost-found-title = Edição guardada em Lost and Found
notify-lost-found-body = A sua versão de { $name } não subiu porque { $reason }. Ela foi guardada em Lost and Found.
notify-lost-found-action = Ver arquivos
notify-folder-action = Ver pastas
notify-folder-missing-title = Uma pasta sincronizada sumiu
notify-folder-missing-body = A pasta "{ $name }" não foi encontrada e parou de sincronizar. Se ela mudou de lugar, escolha o novo local em Preferências.
notify-folder-no-permission-title = Sem permissão para ler uma pasta
notify-folder-no-permission-body = A pasta "{ $name }" parou de sincronizar porque o app não tem permissão para lê-la.
notify-folder-moved-title = Uma pasta sincronizada foi movida
notify-folder-moved-body = A pasta "{ $name }" foi movida e parou de sincronizar. Escolha o novo local em Preferências.
notify-folder-mass-delete-title = Muitos arquivos excluídos de uma vez
notify-folder-mass-delete-body = { $count } itens de "{ $name }" seriam movidos para a lixeira. Nada foi excluído: confirme em Preferências se foi de propósito.
notify-device-ask-title = Fazer backup do dispositivo?
notify-device-ask-body = "{ $name }" foi conectado. Quer fazer backup dos arquivos dele no Google Drive?
notify-device-ask-action = Fazer backup
notify-device-done-title = Backup concluído
notify-device-done-body = O backup de "{ $name }" terminou. Você pode remover o dispositivo com segurança.
notify-crash-title = O app fechou de forma inesperada
notify-crash-body = Enviar o relatório ajuda a corrigir o problema. Ele não leva seus arquivos nem dados de login, e você revisa o texto antes de enviar.
notify-crash-action = Enviar relatório
notify-device-error-title = Falha no backup
notify-device-error-body = O backup de "{ $name }" não terminou: { $error }
notify-device-unreadable = o dispositivo não pôde ser lido
reason-remote-changed = o arquivo foi alterado na nuvem
reason-remote-deleted = o arquivo foi excluído na nuvem
reason-no-permission = você não tem mais permissão para editá-lo
notify-offline-title = Sem conexão
notify-offline-body = O Google Drive volta a sincronizar quando a internet voltar. O que está disponível off-line continua abrindo.
notify-signed-out-title = Entre de novo na conta
notify-signed-out-body = O Google não aceita mais o acesso desta conta. Os arquivos que já estão neste computador continuam abrindo, e o que você gravar sobe depois que você entrar de novo.
notify-signed-out-action = Entrar
notify-disk-low-title = Disco quase cheio
notify-disk-low-body = Restam { $free } MB livres. O Google Drive liberou o que pôde do cache.
notify-quota-title = Armazenamento do Google cheio
notify-quota-body = Arquivos novos e edições não vão subir até você liberar espaço na conta.
notify-upload-failed-title = Falha no envio
notify-upload-failed-body = { $name }: { $error }
notify-shortcut-title = Atalho criado
notify-shortcut-body = Documentos Google não podem ser copiados; no lugar da cópia ficou o atalho { $name }.
notify-paused-title = Sincronização pausada
notify-paused-body = { $name } foi gravado neste computador e sobe quando você retomar a sincronização.
notify-paused-action = Retomar
notify-canceled-title = Envio cancelado
notify-canceled-body = A sua versão de { $name } ficou guardada em Uploads cancelados.
notify-canceled-action = Ver arquivos
notify-moved-shared-title = Arquivo movido para um Drive compartilhado
notify-moved-shared-body = { $name } agora pertence ao Drive compartilhado: quem pode ver o arquivo passa a ser quem tem acesso a ele.
notify-folder-move-title = A pasta não pôde ser movida
notify-folder-move-body = O Google não deixa mover a pasta { $name } entre o Meu Drive e um Drive compartilhado por aqui. Faça isso pelo Google Drive na web.
notify-kind-moved-to-shared-drive = Arquivos movidos para um Drive compartilhado
notify-kind-copied-as-shortcut = Documentos Google copiados como atalho
notify-kind-mount-moved = Pasta do Drive aberta em outro lugar
notify-kind-resume-syncing = Arquivos gravados com a sincronização pausada
notify-kind-offline = Sem conexão com a internet
notify-no-permission-title = Ação recusada no Drive
notify-no-permission-body = Você não tem permissão para { $action } "{ $name }".
no-permission-create = criar
no-permission-move = mover
no-permission-rename = renomear
no-permission-delete = excluir
notify-quota-almost-title = Armazenamento quase cheio
notify-quota-almost-body = A conta já usa { $percent }% do armazenamento. Quando ele encher, arquivos novos e edições deixam de subir.
notify-uploads-stuck-title = Envios parados
notify-uploads-stuck-body = { $count ->
    [1] Um arquivo espera para subir há mais de 3 dias. Veja os detalhes na Atividade.
   *[other] { $count } arquivos esperam para subir há mais de 3 dias. Veja os detalhes na Atividade.
}
notify-local-disk-full-title = Disco do computador cheio
notify-local-disk-full-body = Não há espaço no disco para gravar arquivos do Drive. Libere espaço para continuar salvando.
notify-account-disabled-title = Conta bloqueada para o Drive
notify-account-disabled-body = Quem administra esta conta bloqueou o acesso ao Drive. Nada sincroniza até o acesso voltar; fale com o administrador.
notify-items-trashed-title = Itens movidos para a lixeira
notify-items-trashed-body = { $count ->
    [1] Um item saiu do Drive e foi para a lixeira do computador.
   *[other] { $count } itens saíram do Drive e foram para a lixeira do computador.
}
notify-kind-quota-almost-full = Armazenamento quase cheio
notify-kind-uploads-stuck = Envios parados há dias
notify-kind-items-trashed = Itens das pastas do computador movidos para a lixeira
notify-kind-no-permission = Ações recusadas por falta de permissão

# Login e saída da conta
login-rejected = A conta { $email } não é permitida neste computador.
shared-drives-pending = Algum dos drives que você desligou tem arquivos que ainda não foram enviados. Espere os envios terminarem (ou mantenha o drive ligado) e salve de novo.
logout-pending = Há { $count } itens que ainda não estão no Drive (envios pendentes ou cópias no Lost and Found). Escolha terminar os envios, mover os arquivos para uma pasta ou descartá-los; o Lost and Found nunca é apagado.
not-logged-in = Nenhuma conta conectada.
uninstall-signed-in = Desconecte a conta antes de apagar os dados deste computador: a saída da conta decide o que fazer com os envios pendentes.
path-not-in-drive = { $path } não está no Meu Drive nem num Drive compartilhado.

# CLI provisória
cli-client-id = Client ID:
cli-client-secret = Segredo do cliente:
cli-client-guide = Para criar o seu Client ID, siga docs/guia-client-id.md.
cli-open-url = Abra este endereço no navegador para entrar na conta:
cli-login-ok = Conta conectada: { $email }
cli-login-failed = Não foi possível entrar: { $error }
cli-service-gone = O serviço encerrou a conexão.
cli-uninstall-confirm = Desinstalar o Google Drive deste computador? [s/N]
cli-uninstall-unknown = Não foi possível saber como o app foi instalado (nem por pacote nem pelo install.sh): rode a desinstalação pelo programa instalado.
cli-uninstall-cancelled = Desinstalação cancelada: a senha de administrador não foi confirmada.
cli-uninstall-failed = A remoção do pacote falhou ({ $error }).
cli-uninstall-no-service = Aviso: o serviço não respondeu, então a pasta, o início automático e os dados podem ter ficado. { $error }
cli-uninstall-done = O Google Drive foi desinstalado.
cli-no-service = O serviço do Google Drive não está rodando ({ $error }).
cli-status-account = Conta: { $account }
cli-status-mount = Pasta: { $path }
cli-status-pending = Envios pendentes: { $count }
cli-status-paused = Sincronização pausada
cli-status-offline = Sem conexão
cli-status-last-sync = Última sincronização: { $when }
cli-status-errors = Falhas: { $count }
cli-not-pointer = { $path } não é um ponteiro de Documento Google.
cli-menu-link = Link do item (copie daqui: não há programa para a área de transferência)
cli-menu-failed = Não foi possível fazer a ação
notify-cache-moving-title = Mudando o cache de lugar
notify-cache-moving-body = Os arquivos baixados estão indo para { $path }. O Drive abre quando terminar.
notify-cache-moved-title = Cache no lugar novo
notify-cache-moved-body = Os arquivos baixados agora ficam em { $path }.
notify-link-copied-title = Link copiado
notify-link-restricted = Só as pessoas com acesso podem abrir este link.
notify-link-anyone = Qualquer pessoa com o link pode { $role }.
notify-link-domain = Qualquer pessoa de { $domain } com o link pode { $role }.
link-role-reader = ver
link-role-commenter = comentar
link-role-writer = editar
tray-synced = Tudo sincronizado
tray-syncing = { $count ->
    [1] Sincronizando 1 arquivo
   *[other] Sincronizando { $count } arquivos
}
tray-offline = { $count ->
    [0] Sem internet
   *[other] Sem internet ({ $count } para sincronizar)
}
tray-paused = { $count ->
    [0] Pausado
   *[other] Pausado ({ $count } para sincronizar)
}
tray-quota-full = Armazenamento cheio
tray-needs-sign-in = Entre de novo na conta
tray-error = { $count ->
    [1] 1 arquivo não foi sincronizado
   *[other] { $count } arquivos não foram sincronizados
}
tray-signed-out = Você não fez login
tray-starting = Conectando à sua conta…
tray-accounts = Conectado com { $count } contas
tray-no-service = O serviço do Google Drive não está rodando
tray-open-folder = Abrir a pasta do Google Drive
tray-open-web = Abrir o Google Drive no navegador
tray-search = Pesquisar
tray-pause = Pausar a sincronização
tray-resume = Retomar a sincronização
tray-offline-files = Arquivos off-line
tray-errors = { $count ->
    [0] Lista de erros
   *[other] Lista de erros ({ $count })
}
tray-preferences = Preferências
tray-about = Sobre
tray-feedback = Enviar feedback
tray-diagnostics = Gerar informações de diagnóstico
tray-quit = Sair
tray-sign-in = Fazer login
tray-start-service = Iniciar o serviço
tray-hotkey-search = Abrir a pesquisa do Google Drive

# Títulos das janelas (gdl-ui)
window-home = Google Drive
window-onboarding = Fazer login
window-preferences = Preferências
window-search = Pesquisar
window-errors = Lista de erros
window-offline = Arquivos off-line
window-about = Sobre
window-feedback = Enviar feedback
window-notification-settings = Preferências de notificação
window-uninstall = Desinstalar o Google Drive
window-share = Compartilhar
window-versions = Gerenciar versões
window-shared-drives = Drives compartilhados
window-shortcut = Adicionar atalho ao Drive

## Menu "Google Drive" do botão direito nos gerenciadores de arquivos.
menu-open-web = Abrir no navegador
menu-copy-link = Copiar link
menu-add-shortcut = Adicionar atalho ao Drive
menu-open-docs = Abrir com Documentos Google
menu-open-sheets = Abrir com Planilhas Google
menu-open-slides = Abrir com Apresentações Google
menu-share = Compartilhar
menu-versions = Gerenciar versões
menu-offline = Disponível off-line
menu-online-only = Somente on-line
menu-new-document = Novo Documento Google
menu-new-spreadsheet = Nova Planilha Google
menu-new-presentation = Nova Apresentação Google
menu-refresh = Atualizar pasta
menu-trash = Ver itens na lixeira
menu-new-shared-drive = Novo Drive compartilhado
menu-manage-shared-drives = Gerenciar drives compartilhados
menu-sync-folder = Sincronizar esta pasta com o Google Drive
new-document-name = Documento sem título
new-spreadsheet-name = Planilha sem título
new-presentation-name = Apresentação sem título
