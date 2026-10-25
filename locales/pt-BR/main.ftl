# Pastas virtuais da montagem
folder-my-drive = Meu Drive
folder-trash = .Lixeira do Drive
folder-lost-found = .Lost and Found
folder-shortcut-targets = .shortcut-targets-by-id

# Montagem
mount-moved = A pasta do Drive foi aberta em { $path }

# Página mostrada no navegador ao fim do login
login-page-title = Google Drive para Linux
login-success = Pronto! Pode fechar esta janela e voltar ao app.
login-failure = Não foi possível entrar na conta. Feche esta janela e tente de novo no app.

# Notificações
notify-title = Google Drive
notify-lost-found-title = Edição guardada em Lost and Found
notify-lost-found-body = A sua versão de { $name } não subiu porque { $reason }. Ela foi guardada em Lost and Found.
notify-lost-found-action = Ver arquivos
reason-remote-changed = o arquivo foi alterado na nuvem
reason-remote-deleted = o arquivo foi excluído na nuvem
reason-no-permission = você não tem mais permissão para editá-lo
notify-offline-title = Sem conexão
notify-offline-body = O Google Drive volta a sincronizar quando a internet voltar. O que está disponível off-line continua abrindo.
notify-disk-low-title = Disco quase cheio
notify-disk-low-body = Restam { $free } MB livres. O Google Drive liberou o que pôde do cache.
notify-quota-title = Armazenamento do Google cheio
notify-quota-body = Arquivos novos e edições não vão subir até você liberar espaço na conta.
notify-upload-failed-title = Falha no envio
notify-upload-failed-body = { $name }: { $error }
notify-shortcut-title = Atalho criado
notify-shortcut-body = Documentos Google não podem ser copiados; no lugar da cópia ficou o atalho { $name }.

# Login e saída da conta
login-rejected = A conta { $email } não é permitida neste computador.
logout-pending = Há { $count } itens que ainda não estão no Drive (envios pendentes ou cópias no Lost and Found). Escolha terminar os envios, mover os arquivos para uma pasta ou descartá-los; o Lost and Found nunca é apagado.
not-logged-in = Nenhuma conta conectada.
path-not-in-drive = { $path } não está no Meu Drive.

# CLI provisória
cli-client-id = Client ID:
cli-client-secret = Segredo do cliente:
cli-client-guide = Para criar o seu Client ID, siga docs/guia-client-id.md.
cli-open-url = Abra este endereço no navegador para entrar na conta:
cli-login-ok = Conta conectada: { $email }
cli-login-failed = Não foi possível entrar: { $error }
cli-service-gone = O serviço encerrou a conexão.
cli-no-service = O serviço do Google Drive não está rodando ({ $error }).
cli-status-account = Conta: { $account }
cli-status-mount = Pasta: { $path }
cli-status-pending = Envios pendentes: { $count }
cli-status-paused = Sincronização pausada
cli-status-offline = Sem conexão
cli-status-last-sync = Última sincronização: { $when }
cli-status-errors = Falhas: { $count }
cli-not-pointer = { $path } não é um ponteiro de Documento Google.
