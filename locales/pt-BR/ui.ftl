# Textos das telas da janela (gdl-ui), formatados pelo i18n.js.

service-unavailable = O serviço não está respondendo. Tentando de novo…
screen-not-ready = Esta tela ainda está sendo feita.
action-close = Fechar

about-name = gdrive-desktop-linux
about-version = Versão { $version }
about-license = Software livre, distribuído sob a licença GPL-3.0.
about-unofficial = Cliente não oficial do Google Drive: este projeto não tem ligação com o Google nem é aprovado por ele.
about-project = Página do projeto: { $url }

action-pause = Pausar
action-resume = Retomar

header-signed-out = Você não fez login
header-synced = Sincronização concluída
header-error = Sincronização incompleta
header-syncing = { $count ->
    [one] Sincronizando 1 item
   *[other] Sincronizando { $count } itens
}
header-offline = Sem internet
header-quota-full = Armazenamento cheio
header-paused = Sincronização pausada
header-last-sync-now = Última sincronização há menos de um minuto
header-last-sync = Última sincronização { $ago }
header-never-synced = Ainda não houve sincronização
header-remaining = Cerca de { $time } restantes
header-error-detail = { $count ->
    [one] 1 item não foi sincronizado
   *[other] { $count } itens não foram sincronizados
}
header-offline-detail = As alterações serão enviadas quando a conexão voltar
header-quota-detail = Libere espaço no Drive para continuar sincronizando

showing = Mostrando { $from }–{ $to } de { $total }

action-cancel = Cancelar
action-retry = Tentar de novo
sign-in = Fazer login
search-placeholder = Pesquisar no Google Drive
account-menu = Conta
more-options = Mais opções

view-start = Início
view-activity = Atividade de sincronização
view-offline = Arquivos off-line
view-shared = Compartilhados comigo
view-notifications = Notificações
view-links = Links rápidos

home-signed-out = Faça login para ver seus arquivos e acompanhar a sincronização.

card-attention = Precisa de atenção
card-attention-none = Nada precisa da sua atenção agora.
card-attention-errors = { $count ->
    [one] 1 item não foi sincronizado.
   *[other] { $count } itens não foram sincronizados.
}
card-attention-open = Ver a lista de erros
card-offline = Arquivos disponíveis off-line
card-offline-text = { $count ->
    [0] Nenhum item off-line
    [one] 1 item off-line ({ $size })
   *[other] { $count } itens off-line ({ $size })
}
card-offline-open = Ver arquivos off-line
card-shared = Compartilhados comigo
card-shared-text = { $count ->
    [0] Ninguém compartilhou arquivos com você
    [one] 1 item compartilhado com você
   *[other] { $count } itens compartilhados com você
}
card-shared-open = Ver compartilhados
card-sync = Status de sincronização
card-sync-open = Ver atividade
card-gemini = Gemini
card-gemini-text = Converse sobre seus arquivos do Drive com o Gemini, na web.
card-gemini-open = Abrir o Gemini

activity-empty = Nenhuma atividade de sincronização ainda.
activity-upload-queued = Na fila para enviar
activity-uploading = Enviando
activity-uploading-percent = Enviando ({ $percent }%)
activity-uploaded = Enviado
activity-upload-canceling = Cancelando o envio
activity-upload-canceled = Envio cancelado
activity-upload-failed = Falha no envio
activity-downloading = Baixando
activity-downloaded = Baixado
activity-trashing = Indo para a lixeira
activity-trashed = Na lixeira
activity-restoring = Restaurando
activity-restored = Restaurado
column-name = Nome
column-size = Tamanho
column-date = Data
column-status = Status
page-size = Itens por página
page-previous = Página anterior
page-next = Próxima página
row-menu = Ações do item
row-open-folder = Abrir no gerenciador de arquivos
row-open-web = Abrir na web
row-copy-link = Copiar link
row-cancel-upload = Cancelar upload
link-copied = Link copiado
upload-canceled = Envio cancelado. A versão local está em "Uploads cancelados".
upload-finished-first = O envio terminou antes de ser cancelado.

offline-empty = Nenhum arquivo está disponível off-line.
offline-manage = Gerenciar arquivos off-line

shared-empty = Nenhum item foi compartilhado com você.
shared-by = { $owner } · { $date }
shared-add-shortcut = Adicionar atalho ao Drive
shortcut-added = Atalho adicionado ao Meu Drive

notifications-empty = Nenhuma notificação.
notification-dismiss = Dispensar

link-drive-web = Google Drive na web
link-faq = Perguntas frequentes
link-help = Ajuda
link-privacy = Política de Privacidade
link-terms = Termos de uso

account-usage = Usando { $used } de { $total }
account-usage-unlimited = Usando { $used }
account-disconnect = Desconectar a conta
storage-manage = Gerenciar armazenamento

more-errors = { $count ->
    [0] Lista de erros
   *[other] Lista de erros ({ $count })
}
more-notification-settings = Preferências de notificação
more-reset = Redefinir as preferências
more-preferences = Preferências
more-about = Sobre
more-quit = Sair

reset-title = Redefinir as preferências?
reset-body = Tudo o que você mudou nas preferências volta ao padrão. As definidas pelo administrador continuam valendo.
reset-confirm = Redefinir
reset-done = Preferências redefinidas

quit-title = Sair do Google Drive?
quit-body = { $count ->
    [one] Ainda há 1 item para sincronizar. Ele será sincronizado da próxima vez que o app abrir.
   *[other] Ainda há { $count } itens para sincronizar. Eles serão sincronizados da próxima vez que o app abrir.
}

disconnect-title = Desconectar a conta?
disconnect-body = A conta { $account } será desconectada deste computador.
disconnect-offline-warning = Os arquivos off-line e o cache desta conta serão removidos do computador.
disconnect-pending = { $count ->
    [one] 1 arquivo ainda não foi sincronizado. O que fazer com ele?
   *[other] { $count } arquivos ainda não foram sincronizados. O que fazer com eles?
}
disconnect-finish = Terminar de sincronizar e depois desconectar
disconnect-move = { $count ->
    [one] Mover o arquivo não sincronizado para uma pasta
   *[other] Mover os { $count } arquivos não sincronizados para uma pasta
}
disconnect-discard = { $count ->
    [one] Excluir o arquivo não sincronizado
   *[other] Excluir os { $count } arquivos não sincronizados
}
disconnect-discard-sure = Entendo que esses arquivos serão perdidos
disconnect-pick-folder = Escolha a pasta para os arquivos não sincronizados
disconnect-progress = Desconectando…
disconnect-done = Conta desconectada
