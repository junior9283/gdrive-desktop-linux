# pkexec a partir do serviço do usuário

Data: 2026-11-07. Máquina: Nobara (Fedora 44), KDE Plasma.

Pergunta (desenho da atualização automática): um `pkexec` disparado por um processo de uma unidade
do systemd do usuário, fora da sessão gráfica, encontra o agente do polkit e mostra a senha?

Teste:

    systemd-run --user --wait --collect --unit=teste-pkexec pkexec /usr/bin/true

Resultado: a janela de senha do KDE apareceu; com a senha, saída 0 (3,1 s de execução).

Decisão: o serviço (`gdl-daemon`) roda o instalador ele mesmo. O caminho alternativo ("a janela
executa") fica descartado. GNOME não foi testado; se lá falhar, o `pkexec` sai com 127 e o app
mostra o aviso de falha, sem instalar nada.
