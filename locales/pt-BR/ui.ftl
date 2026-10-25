# Textos das telas da janela (gdl-ui), formatados pelo i18n.js.

service-unavailable = O serviço não está respondendo. Tentando de novo…
screen-not-ready = Esta tela ainda está sendo feita.
action-close = Fechar

about-name = gdrive-desktop-linux
about-version = Versão { $version }
about-license = Software livre, distribuído sob a licença GPL-3.0.
about-unofficial = Cliente não oficial do Google Drive: este projeto não tem ligação com o Google nem é aprovado por ele.
about-third-party = Softwares de terceiros ({ $count })
about-license-file = licença no pacote
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

# Primeiro uso
onboarding-steps = Passos
onboarding-step-sign-in = Entrar na conta
onboarding-step-waiting = Login no navegador
onboarding-step-intro = Conheça o app
onboarding-step-files = Seus arquivos
onboarding-step-offline = Uso sem internet
onboarding-skip = Pular
onboarding-back = Voltar
onboarding-continue = Continuar
onboarding-finish = Concluir
onboarding-sign-in-title = Entre com a sua conta Google
onboarding-sign-in-text = Nesta versão, o app usa um Client ID que você mesmo cria no Google Cloud, de graça. O guia mostra o passo a passo, em uns 10 minutos.
onboarding-client-guide = Como criar o meu Client ID
onboarding-client-id = Client ID
onboarding-client-secret = Chave secreta do cliente
onboarding-client-note = Os dois ficam guardados no chaveiro do sistema, junto com o login.
onboarding-missing-client = Preencha o Client ID e a chave secreta para continuar.
onboarding-login-failed = O login não deu certo: { $error }
onboarding-waiting-title = Termine o login no navegador
onboarding-waiting-text = O navegador abriu na página de login do Google. Entre na conta e autorize o acesso; esta janela segue sozinha. O pedido vale por 5 minutos.
onboarding-waiting-other = O navegador não abriu, ou você quer usar outro? Gere o link e cole onde preferir.
onboarding-generate-link = Gerar link
onboarding-link-copied = Link copiado
onboarding-intro-title = Seu Drive, direto no computador
onboarding-intro-stream-title = Arquivos quando você precisa
onboarding-intro-stream-text = Tudo aparece numa pasta, mas só ocupa espaço o que você abre.
onboarding-intro-sync-title = Sempre em dia
onboarding-intro-sync-text = O que você muda aqui chega ao Drive, e o que muda no Drive aparece aqui.
onboarding-intro-offline-title = Também sem internet
onboarding-intro-offline-text = Deixe guardados no computador os arquivos que você quer ter sempre à mão.
onboarding-files-title = Veja seus arquivos no Dolphin
onboarding-files-text = O Google Drive é uma pasta em { $path }. Abra no Dolphin e use como qualquer outra.
onboarding-open-folder = Abrir a pasta
onboarding-offline-title = Arquivos disponíveis off-line
onboarding-offline-text = Para usar um arquivo ou uma pasta sem internet, rode no terminal:
onboarding-offline-command = gdrive-desktop-linux pin <caminho>
onboarding-offline-list = Os itens off-line aparecem em "Arquivos off-line", na janela principal.

# Preferências
pref-tab-computer = Meu computador
pref-tab-drive = Google Drive
pref-tab-settings = Configurações
pref-computer-empty = Nenhuma pasta do computador está em backup no Google Drive.
pref-add-folder = Adicionar pasta
pref-next-version = Chega na versão { $version }
pref-drive-mode = Como o Meu Drive fica no computador
pref-stream = Fazer streaming dos arquivos
pref-stream-text = Os arquivos ficam na nuvem e são baixados quando você abre. Ocupa pouco espaço no disco.
pref-mirror = Espelhar os arquivos
pref-mirror-text = Uma cópia de tudo fica no computador e outra na nuvem.
pref-drive-folder = Pasta do Google Drive
pref-open-folder = Abrir a pasta
pref-section-location = Local dos arquivos
pref-mount = Local do streaming
pref-sidebar = Mostrar em Locais do Dolphin
pref-section-cache = Cache
pref-cache-path = Pasta do cache
pref-change = Alterar
pref-reset = Redefinir
pref-pick-folder = Escolha a pasta
pref-default = Padrão
pref-cache-limit = Limite do cache (GB)
pref-cache-limit-hint = Vazio: o app decide. De 1 a { $max } GB.
pref-cache-storage = Ver armazenamento do cache
pref-section-system = Sistema
pref-autostart = Iniciar o app ao fazer login
pref-metered = Pausar a sincronização em conexão limitada
pref-theme = Tema
pref-theme-system = Do sistema
pref-theme-light = Claro
pref-section-network = Rede
pref-proxy = Proxy
pref-proxy-auto = Detectar automaticamente (do sistema)
pref-proxy-direct = Conexão direta
pref-proxy-manual = Manual
pref-proxy-server = Endereço do proxy
pref-bandwidth-rx = Limitar o download (KB/s)
pref-bandwidth-tx = Limitar o upload (KB/s)
pref-section-other = Outras
pref-hotkey = Atalho da pesquisa
pref-hotkey-hint = Clique no campo e pressione a combinação de teclas.
pref-error-reports = Enviar relatórios de erro automaticamente
pref-error-reports-hint = Nesta versão, o app só guarda a sua escolha: nada é enviado.
pref-account = Conta
pref-managed = Gerenciado pelo administrador
pref-pending = Alterações pendentes
pref-save = Salvar
pref-restart-needed = Algumas alterações só valem depois de reiniciar o app.
pref-restart-now = Reiniciar agora
pref-saved = Preferências salvas
pref-invalid-cache = Use um número inteiro de 1 a { $max }.
pref-invalid-bandwidth = Use um número inteiro maior que zero.
pref-invalid-proxy = Use um endereço como http://servidor:porta (http, https, socks5 ou socks5h).

# Preferências de notificação
notification-settings-title = Preferências de notificação
notification-settings-text = Escolha quais avisos o app mostra. Os de erro grave e de conta sempre aparecem.
notification-settings-empty = Nenhum aviso pode ser desligado.
