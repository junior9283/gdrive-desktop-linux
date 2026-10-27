// O submenu "Google Drive" do botão direito no Dolphin. Quais itens aparecem, com quais nomes e
// marcas, e o que cada um faz é decidido pelo serviço (métodos `Menu` e `MenuAction`); aqui só
// se desenha a resposta. Fora do Meu Drive, ou com o serviço parado, não aparece nada.

#include <KAbstractFileItemActionPlugin>
#include <KFileItemListProperties>
#include <KPluginFactory>

#include <QAction>
#include <QClipboard>
#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusMessage>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QGuiApplication>
#include <QIcon>
#include <QMenu>

#include "gdriveservice.h"

using namespace gdrive;

namespace
{
const QString APP_ICON = QStringLiteral("io.github.junior9283.GDriveDesktopLinux");
// O menu abre na hora: o serviço responde do banco local, e passando disso ele fica de fora.
constexpr int MENU_TIMEOUT_MS = 500;

struct Entry {
    QString id;
    QString label;
    QString state;
};

QList<Entry> readEntries(const QDBusArgument &arg)
{
    QList<Entry> entries;
    arg.beginArray();
    while (!arg.atEnd()) {
        Entry entry;
        arg.beginStructure();
        arg >> entry.id >> entry.label >> entry.state;
        arg.endStructure();
        entries.append(entry);
    }
    arg.endArray();
    return entries;
}
}

class GDriveMenu : public KAbstractFileItemActionPlugin
{
    Q_OBJECT

public:
    GDriveMenu(QObject *parent, const QVariantList &)
        : KAbstractFileItemActionPlugin(parent)
    {
    }

    QList<QAction *> actions(const KFileItemListProperties &items, QWidget *parentWidget) override
    {
        if (!items.isLocal()) {
            return {};
        }
        QStringList paths;
        for (const QUrl &url : items.urlList()) {
            paths.append(url.toLocalFile());
        }
        // Com o serviço parado, a resposta é um erro na hora (perguntar não o inicia).
        const QDBusMessage reply =
            QDBusConnection::sessionBus().call(serviceCall(QStringLiteral("Menu"), {paths}), QDBus::Block, MENU_TIMEOUT_MS);
        if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) {
            return {};
        }
        const QList<Entry> entries = readEntries(reply.arguments().at(0).value<QDBusArgument>());
        if (entries.isEmpty()) {
            return {};
        }

        auto *menu = new QMenu(QStringLiteral("Google Drive"), parentWidget);
        menu->setIcon(QIcon::fromTheme(APP_ICON));
        for (const Entry &entry : entries) {
            if (entry.id == QLatin1String("-")) {
                menu->addSeparator();
                continue;
            }
            QAction *action = menu->addAction(entry.label);
            if (!entry.state.isEmpty()) {
                action->setCheckable(true);
                action->setChecked(entry.state == QLatin1String("on"));
            }
            const QString id = entry.id;
            connect(action, &QAction::triggered, this, [this, id, paths]() {
                run(id, paths);
            });
        }
        return {menu->menuAction()};
    }

private:
    // Sem esperar: o serviço pode demorar (criar um documento vai ao Drive).
    void run(const QString &id, const QStringList &paths)
    {
        const auto call = serviceCall(QStringLiteral("MenuAction"), {id, paths});
        auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(call), this);
        connect(watcher, &QDBusPendingCallWatcher::finished, this, [this](QDBusPendingCallWatcher *done) {
            const QDBusPendingReply<QString> reply = *done;
            done->deleteLater();
            if (reply.isError()) {
                Q_EMIT error(reply.error().message());
            } else if (!reply.value().isEmpty()) {
                QGuiApplication::clipboard()->setText(reply.value());
            }
        });
    }
};

K_PLUGIN_CLASS_WITH_JSON(GDriveMenu, "gdrivemenu.json")

#include "gdrivemenu.moc"
