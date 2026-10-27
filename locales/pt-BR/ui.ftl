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
header-starting = Conectando à sua conta…
header-synced = Sincronização concluída
header-error = Sincronização incompleta
header-syncing = { $count ->
    [one] Sincronizando 1 item
   *[other] Sincronizando { $count } itens
}
header-offline = Sem internet
header-quota-full = Armazenamento cheio
header-needs-sign-in = Entre de novo na conta
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
home-starting = Conectando às suas contas. Isso leva só alguns segundos.

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
upload-in-use = O arquivo ainda está aberto em outro programa. Feche-o e tente de novo.

offline-empty = Nenhum arquivo está disponível off-line.
offline-manage = Gerenciar arquivos off-line

shared-empty = Nenhum item foi compartilhado com você.
shared-by = { $owner } · { $date }
shared-add-shortcut = Adicionar atalho ao Drive
shortcut-title = Adicionar atalho ao Drive
shortcut-text = Escolha a pasta onde o atalho vai ficar. O item continua onde está; o atalho só aponta para ele.
shortcut-places = Locais
shortcut-no-subfolders = Esta pasta não tem subpastas.
shortcut-no-places = Não há pasta onde esta conta possa criar o atalho.
shortcut-add = Adicionar atalho
shortcut-missing = Nenhum item para criar o atalho.

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
account-add = Adicionar outra conta
account-sign-in-again = Entrar de novo
account-needs-sign-in = Precisa entrar de novo
account-disconnect-all = Desconectar todas as contas
disconnect-all-title = Desconectar todas as contas?
disconnect-all-body = Todas as contas ({ $count }) serão desconectadas deste computador.
disconnect-all-done = Contas desconectadas
onboarding-resume-title = Entre de novo na conta
onboarding-resume-text = O login de { $account } venceu. Entre de novo com a mesma conta: os arquivos que estão no computador continuam aqui, e o que ficou na fila segue quando o login voltar.
onboarding-step-add = Adicionar conta
onboarding-step-resume = Entrar de novo
onboarding-add-title = Adicione outra conta Google
onboarding-add-text = O navegador vai abrir na página de login do Google. Entre com a outra conta: ela ganha a própria pasta, e as duas sincronizam ao mesmo tempo. O Client ID da primeira conta é reaproveitado.
onboarding-login-failed-already-connected = Esta conta já está conectada.
onboarding-login-failed-other-account = O login foi feito com outra conta. Entre com a conta que precisava entrar de novo.
onboarding-login-failed-too-many-accounts = O máximo de 4 contas já foi atingido. Desconecte uma antes de adicionar outra.
onboarding-login-failed-add-account-disabled = O administrador não permite adicionar outra conta.
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
onboarding-step-setup = Criar o Client ID
onboarding-setup-title = Crie o seu Client ID no Google Cloud
onboarding-setup-text = Nesta versão, o app não tem um login próprio no Google: você cria o seu, de graça. Leva uns 10 minutos e só precisa ser feito uma vez. Siga as partes em ordem; cada botão abre a página certa no navegador. Já tem um Client ID? Clique em Continuar.
onboarding-setup-open = Abrir no Google Cloud
onboarding-setup-copy = Copiar
onboarding-setup-copied = Copiado
onboarding-setup-project-title = Crie um projeto
onboarding-setup-project-text = Entre com a conta Google que você vai usar no app. Dê um nome ao projeto (por exemplo, Drive no Linux) e clique em Criar. Nas próximas partes, confira no alto da página se esse projeto está selecionado.
onboarding-setup-drive-api-title = Ative a Google Drive API
onboarding-setup-drive-api-text = Na página da Google Drive API, clique em Ativar. Sem isso o app não consegue ver seus arquivos.
onboarding-setup-branding-title = Configure a tela de consentimento
onboarding-setup-branding-text = Clique em Começar e preencha o nome do app (por exemplo, Drive no Linux) e o seu e-mail de suporte. Em Público-alvo, escolha Externo; informe o e-mail de contato, aceite a política e clique em Criar. Depois, na página Branding: não envie logotipo (com logotipo, o Google exige uma verificação demorada; se já enviou, clique em Remover). Em Domínio do app, cole a página inicial, a política de privacidade e os termos abaixo; em Domínios autorizados, clique em Adicionar domínio e cole o domínio. Clique em Salvar no fim da página. Sem esses campos, o Google não deixa publicar o app (parte 5).
onboarding-setup-field-homepage = Página inicial
onboarding-setup-field-privacy = Política de privacidade
onboarding-setup-field-terms = Termos de serviço
onboarding-setup-field-domain = Domínio autorizado
onboarding-setup-scopes-title = Escolha as permissões
onboarding-setup-scopes-text = Em Acesso a dados, clique em Adicionar ou remover escopos. No fim do painel, na caixa Adicionar escopos manualmente, cole os três abaixo (separados por vírgula), clique em Adicionar à tabela e confira se os três ficaram marcados. Clique em Atualizar e, de volta à página, em Salvar no fim dela; sem o Salvar, nada fica gravado. Ao final, email e openid aparecem em Escopos não confidenciais e o drive em Seus escopos restritos.
onboarding-setup-audience-title = Publique o app
onboarding-setup-audience-text = Em Público-alvo, clique em Publicar app e depois em Confirmar. Se aparecer "Para publicar o app, conclua a configuração na página de branding", volte à parte 3 e preencha os campos de Domínio do app. Depois de publicar, o Google mostra "Seu aplicativo precisa ser verificado": pode ignorar, o app funciona sem a verificação (até 100 contas). Não deixe "em teste": nesse modo o Google desconecta você a cada 7 dias, e só entram as contas adicionadas em Usuários de teste.
onboarding-setup-client-title = Crie a credencial
onboarding-setup-client-text = Em Tipo de aplicativo, escolha App para computador, dê um nome e clique em Criar. Copie o ID do cliente e a chave secreta e cole no próximo passo. Guarde-os só no app: não os envie a ninguém.
onboarding-trouble-title = Problemas comuns
onboarding-trouble-publish-blocked = "Para publicar o app, conclua a configuração na página de branding"
onboarding-trouble-publish-blocked-fix = Faltam a página inicial, a política de privacidade ou o domínio autorizado, ou há um logotipo enviado. Preencha e remova o logotipo na parte 3, clique em Salvar e tente publicar de novo.
onboarding-trouble-access-denied = "access_denied" ou "acesso recusado"
onboarding-trouble-access-denied-fix = Você clicou em Cancelar, ou o app ficou "em teste" e a sua conta não está em Usuários de teste. Publique o app (parte 5) ou adicione a sua conta em Público-alvo → Usuários de teste e tente de novo.
onboarding-trouble-redirect-mismatch = "redirect_uri_mismatch"
onboarding-trouble-redirect-mismatch-fix = A credencial não é do tipo App para computador. Crie uma nova (parte 6).
onboarding-trouble-weekly-logout = O login para de funcionar depois de uma semana
onboarding-trouble-weekly-logout-fix = O app ficou "em teste". Publique o app (parte 5) e entre de novo.
onboarding-trouble-api-disabled = "Google Drive API has not been used in project…"
onboarding-trouble-api-disabled-fix = A Drive API não foi ativada no projeto (parte 2).
onboarding-waiting-unverified = Se aparecer "O Google não verificou este app", clique em Avançado e depois em Acessar: o aviso existe porque o app é seu e não passou pela revisão do Google. Seus dados só trafegam entre este computador e o Google.
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
onboarding-sign-in-text = Cole aqui o ID do cliente e a chave secreta que você criou no passo anterior. Ainda não tem? Clique em Voltar e siga o guia.
onboarding-client-guide = Abrir o guia completo no site do projeto
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
pref-computer-text = As pastas escolhidas aqui ficam em "Computadores" no Google Drive, e as mudanças seguem nos dois sentidos.
folder-open = Abrir pasta
folder-stop = Parar de sincronizar
folder-stop-title = Parar de sincronizar "{ $name }"?
folder-stop-body = Os arquivos continuam neste computador e no Google Drive; só as mudanças deixam de ser sincronizadas.
folder-stopped = A pasta "{ $name }" não é mais sincronizada.
folder-state-stopped-missing = Parada: a pasta não foi encontrada.
folder-state-stopped-no-permission = Parada: sem permissão para ler a pasta.
folder-state-stopped-moved = Parada: a pasta foi movida.
folder-state-confirm-deletes = { $count ->
    [one] Esperando sua confirmação para mover 1 item para a lixeira.
   *[other] Esperando sua confirmação para mover { $count } itens para a lixeira.
}
folder-confirm-deletes = Mover para a lixeira
folder-config-title = Adicionar pasta
folder-config-account = Conta
folder-config-summary = { $files ->
    [one] 1 arquivo, { $size }
   *[other] { $files } arquivos, { $size }
}
folder-config-sync = Sincronizar com o Google Drive
folder-config-merge = Juntar com a pasta de mesmo nome que já está em "Computadores"
folder-config-done = Concluído
folder-added = A pasta "{ $name }" está sendo sincronizada.
folder-refused-missing = A pasta não existe.
folder-refused-unreadable = Não foi possível ler a pasta.
folder-refused-nested = Esta pasta já está dentro de uma pasta sincronizada ou da pasta do Google Drive.
folder-refused-contains = Esta pasta contém uma pasta já sincronizada ou a pasta do Google Drive.
folder-refused-network = Pastas de rede não podem ser sincronizadas.
folder-refused-folders-disabled = O administrador desativou a sincronização de pastas do computador.
pref-coming-soon = Chega numa próxima versão
pref-drive-mode = Como o Meu Drive fica no computador
pref-stream = Fazer streaming dos arquivos
pref-stream-text = Os arquivos ficam na nuvem e são baixados quando você abre. Ocupa pouco espaço no disco.
pref-mirror = Espelhar os arquivos
pref-mirror-text = Uma cópia de tudo fica no computador e outra na nuvem.
pref-drive-folder = Pasta do Google Drive
pref-open-folder = Abrir a pasta
pref-shared-drives = Gerenciar drives compartilhados
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

# Pesquisa rápida
search-recent = Recentes
search-none = Nenhum item corresponde a "{ $query }".
search-failed = A pesquisa não deu certo: { $error }

# Lista de erros
errors-title = Erros de sincronização
errors-empty = Nenhum erro de sincronização.
errors-done = Concluído
errors-count = { $count ->
    [one] 1 item
   *[other] { $count } itens
}
error-category-quota = Armazenamento cheio
error-category-quota-text = Libere espaço no Google Drive ou aumente o armazenamento: os envios continuam depois.
error-category-upload-limit = Limite diário de upload atingido
error-category-upload-limit-text = O Google Drive aceita até 750 GB de envios por dia. Os envios continuam amanhã.
error-category-no-permission = Sem permissão para editar
error-category-no-permission-text = A sua versão ficou guardada em Lost and Found.
error-category-too-large = Arquivo grande demais
error-category-too-large-text = O arquivo passa do tamanho que o Google Drive aceita.
error-category-cloud-missing = O item não existe mais no Google Drive
error-category-cloud-missing-text = Ele foi excluído ou deixou de ser compartilhado com você.
error-category-local-missing = O arquivo não existe mais no computador
error-category-local-missing-text = Ele foi apagado ou movido antes de subir.
error-category-lost-and-found = Guardados em Lost and Found
error-category-lost-and-found-text = Versões locais que não puderam subir, guardadas para você revisar.
error-category-unknown = Outros erros
error-category-unknown-text = Tente de novo; se continuar, veja o erro técnico de cada item.

# Arquivos off-line
offline-title = Arquivos off-line
offline-usage = Uso do disco
offline-usage-offline = Off-line
offline-usage-cache = Cache
offline-usage-free = Livre
offline-state-on = Disponível off-line
offline-state-mixed = Parte do conteúdo off-line
offline-remove = Remover acesso off-line
offline-removed = Acesso off-line removido
offline-folder-empty = Nada nesta pasta está disponível off-line.
offline-clear-cache = Limpar cache
offline-cache-cleared = { $size } liberados
offline-clear-offline = Limpar arquivos off-line
offline-clear-title = Limpar os arquivos off-line?
offline-clear-body = Eles deixam de estar disponíveis sem internet e saem do computador. Nada é excluído do Google Drive.
offline-clear-confirm = Limpar
offline-cleared = Arquivos off-line removidos do computador
offline-cache-settings = Configurações de cache
offline-refresh = Atualizar

uninstall-open = Desinstalar…
uninstall-title = Desinstalar o Google Drive?
uninstall-body = Os programas saem deste computador e a pasta do Drive deixa de aparecer. Os arquivos no Google Drive, na nuvem, não são tocados.
uninstall-purge = Apagar também os dados deste computador
uninstall-purge-hint = A conta conectada, o cache, os arquivos off-line, as miniaturas e as preferências. As cópias guardadas em Lost and Found ficam.
uninstall-confirm = Desinstalar
uninstall-progress = Desinstalando… Se o app veio de um pacote, o sistema pede a senha de administrador.
uninstall-waiting = Terminando os envios pendentes antes de desconectar…
uninstall-done = Google Drive desinstalado
uninstall-done-body = Os programas foram removidos deste computador.

share-missing = Abra esta janela pelo menu do Google Drive no gerenciador de arquivos.
share-title = Compartilhar "{ $name }"
share-cannot = Você só pode ver quem tem acesso: mudar o compartilhamento pede permissão de edição.
share-add = Adicionar pessoas
share-add-placeholder = Adicionar pessoas por e-mail
share-role = Papel
share-role-of = Papel de { $name }
share-role-reader = Leitor
share-role-commenter = Comentarista
share-role-writer = Editor
share-role-owner = Proprietário
share-role-remove = Remover acesso
share-notify = Avisar as pessoas por e-mail
share-message = Mensagem (opcional)
share-send = Enviar
share-invalid = Confira estes e-mails: { $emails }
share-added = { $count ->
    [one] Acesso dado a 1 pessoa
   *[other] Acesso dado a { $count } pessoas
}
share-removed = Acesso removido
share-people = Pessoas com acesso
share-general = Acesso geral
share-general-role = Papel de quem tem o link
share-general-restricted = Restrito
share-general-restricted-text = Só as pessoas com acesso podem abrir com o link.
share-general-domain = { $domain }
share-general-domain-text = Qualquer pessoa de { $domain } com o link pode abrir.
share-general-anyone = Qualquer pessoa com o link
share-general-anyone-text = Qualquer pessoa na internet com o link pode abrir.
share-copy-link = Copiar link
share-done = Concluído

versions-missing = Abra esta janela pelo menu do Google Drive no gerenciador de arquivos.
versions-title = Versões de "{ $name }"
versions-text = As versões antigas podem ser apagadas depois de 30 dias ou de 100 versões, salvo as marcadas para manter para sempre.
versions-current = Atual
versions-kept = Mantida para sempre
versions-actions = Ações da versão
versions-download = Baixar
versions-downloaded = Salva em { $path }
versions-restore = Restaurar
versions-restore-title = Restaurar esta versão?
versions-restore-body = O arquivo volta a ter o conteúdo de { $date }. A versão atual continua na lista.
versions-restored = Versão restaurada; ela sobe como versão nova
versions-keep = Manter para sempre
versions-unkeep = Não manter para sempre
versions-kept-done = A versão será mantida para sempre
versions-unkept-done = A versão pode ser apagada com o tempo

shared-drives-title = Drives compartilhados
shared-drives-text = Escolha quais Drives compartilhados aparecem na pasta "Drives compartilhados" deste computador. Os que você desliga somem da pasta e deixam de ser sincronizados.
shared-drives-empty = Esta conta não tem Drives compartilhados.
shared-drives-new = Novo Drive compartilhado
shared-drives-name = Nome do Drive compartilhado
shared-drives-create = Criar
shared-drives-created = Drive compartilhado "{ $name }" criado
shared-drives-saved = Alterações salvas
