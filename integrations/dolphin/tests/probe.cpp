// Monta o menu do botão direito como o Dolphin (`KFileItemActions`, que acha o plugin pelo
// QT_PLUGIN_PATH e pelos tipos dos metadados) e imprime o submenu "Google Drive" para os
// caminhos dados; com `--trigger ITEM`, aciona o item e imprime a área de transferência ou o
// erro. Roda sem tela (QT_QPA_PLATFORM=offscreen), contra o serviço falso de `fake_service.py`.
//
// Uso: probe [--trigger ITEM] CAMINHO...

#include <KFileItem>
#include <KFileItemActions>
#include <KFileItemListProperties>

#include <QAction>
#include <QApplication>
#include <QClipboard>
#include <QMenu>
#include <QTimer>

#include <cstdio>

static void out(const QString &line)
{
    std::printf("%s\n", qPrintable(line));
}

int main(int argc, char **argv)
{
    QApplication app(argc, argv);
    QStringList args = app.arguments().mid(1);
    QString trigger;
    if (args.value(0) == QLatin1String("--trigger")) {
        args.takeFirst();
        trigger = args.takeFirst();
    }
    KFileItemList items;
    for (const QString &path : args) {
        items.append(KFileItem(QUrl::fromLocalFile(path)));
    }

    KFileItemActions kio;
    kio.setItemListProperties(KFileItemListProperties(items));
    QMenu popup;
    kio.addActionsTo(&popup);
    QAction *ours = nullptr;
    for (QAction *action : popup.actions()) {
        if (action->menu() && action->text() == QLatin1String("Google Drive")) {
            ours = action;
        }
    }
    if (!ours) {
        out(QStringLiteral("(sem menu)"));
        return 0;
    }
    QMenu *menu = ours->menu();
    out(ours->text());
    QAction *chosen = nullptr;
    for (QAction *action : menu->actions()) {
        if (action->isSeparator()) {
            out(QStringLiteral("  ---"));
            continue;
        }
        const QString mark = !action->isCheckable() ? QString() : action->isChecked() ? QStringLiteral("[x] ") : QStringLiteral("[ ] ");
        out(QStringLiteral("  ") + mark + action->text());
        if (action->text() == trigger) {
            chosen = action;
        }
    }
    if (trigger.isEmpty()) {
        return 0;
    }
    if (!chosen) {
        out(QStringLiteral("sem o item ") + trigger);
        return 2;
    }
    QObject::connect(&kio, &KFileItemActions::error, &app, [&app](const QString &message) {
        out(QStringLiteral("erro: ") + message);
        app.quit();
    });
    QObject::connect(QGuiApplication::clipboard(), &QClipboard::dataChanged, &app, [&app]() {
        out(QStringLiteral("copiado: ") + QGuiApplication::clipboard()->text());
        app.quit();
    });
    chosen->trigger();
    // Ações sem resposta visível (só o serviço muda algo) terminam pelo tempo, folgado para a
    // resposta de erro chegar mesmo com a máquina ocupada.
    QTimer::singleShot(3000, &app, &QApplication::quit);
    return app.exec();
}
