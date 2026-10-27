// Os emblemas de estado nos arquivos do Meu Drive no Dolphin: na nuvem, off-line, sincronizando
// ou em conflito. O estado vem do atributo `user.gdl.state` que a montagem dá a cada item; nas
// pastas do computador sincronizadas (que não têm o atributo), do método `FolderStates` do
// serviço, perguntado só abaixo das raízes que `FolderRoots` dá (um filtro: quem decide é o
// serviço) e sem esperar: o emblema chega depois por `overlaysChanged` e fica guardado. Fora
// disso, nada aparece. Quando o estado muda, o serviço manda os caminhos no sinal
// `StatesChanged` e o emblema é redesenhado.

#include <KOverlayIconPlugin>

#include <QDBusConnection>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDBusServiceWatcher>
#include <QHash>
#include <QFile>
#include <QUrl>

#include <sys/xattr.h>

#include "gdriveservice.h"

using namespace gdrive;

namespace
{
// Os valores de `user.gdl.state` (crates/gdl-fs/src/layout.rs); cada um tem o emblema
// `gdl-emblem-<valor>` em assets/icons/hicolor/scalable/emblems.
const QList<QByteArray> STATES = {"cloud", "offline", "syncing", "conflict"};
// Só ao carregar o plugin a lista de raízes é esperada (o serviço responde do banco local).
constexpr int ROOTS_TIMEOUT_MS = 200;
}

class GDriveOverlay : public KOverlayIconPlugin
{
    Q_OBJECT
    Q_PLUGIN_METADATA(IID "org.kde.overlayicon.gdrivedesktoplinux" FILE "gdriveoverlay.json")

public:
    explicit GDriveOverlay(QObject *parent = nullptr)
        : KOverlayIconPlugin(parent)
    {
        // Sem o serviço no ar, só não chegam atualizações: o atributo continua valendo.
        auto bus = QDBusConnection::sessionBus();
        bus.connect(SERVICE, PATH, INTERFACE, QStringLiteral("StatesChanged"), this, SLOT(statesChanged(QStringList)));
        // As raízes mudam quando uma pasta entra ou sai, ou quando o serviço volta.
        bus.connect(SERVICE, PATH, INTERFACE, QStringLiteral("LocalFoldersChanged"), this, SLOT(loadRoots()));
        auto *watcher = new QDBusServiceWatcher(SERVICE, bus, QDBusServiceWatcher::WatchForRegistration, this);
        connect(watcher, &QDBusServiceWatcher::serviceRegistered, this, &GDriveOverlay::loadRoots);
        setRoots(bus.call(serviceCall(QStringLiteral("FolderRoots")), QDBus::Block, ROOTS_TIMEOUT_MS));
    }

    // A leitura do atributo vem do banco local do serviço (sem rede), por isso cabe aqui.
    QStringList getOverlays(const QUrl &url) override
    {
        if (!url.isLocalFile()) return {};
        const QByteArray path = QFile::encodeName(url.toLocalFile());
        char value[16];
        const ssize_t size = getxattr(path.constData(), "user.gdl.state", value, sizeof value);
        QByteArray state(value, qMax<ssize_t>(size, 0));
        if (size < 0) {
            const QString local = url.toLocalFile();
            if (!inFolder(local)) return {};
            const auto found = known.constFind(local);
            if (found == known.constEnd()) {
                askState(url);
                return {};
            }
            state = *found;
        }
        if (!STATES.contains(state)) return {};
        return {QStringLiteral("gdl-emblem-") + QString::fromLatin1(state)};
    }

private:
    QStringList roots;
    // O estado de cada caminho das pastas, como o serviço respondeu (vazio enquanto pergunta).
    QHash<QString, QByteArray> known;

    void askState(const QUrl &url)
    {
        const QString path = url.toLocalFile();
        known.insert(path, {});
        const auto call = serviceCall(QStringLiteral("FolderStates"), {QStringList{path}});
        auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(call), this);
        connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, url, path](QDBusPendingCallWatcher *done) {
            const QDBusPendingReply<QStringList> reply = *done;
            done->deleteLater();
            if (reply.isError() || !known.contains(path)) return;
            known.insert(path, reply.value().value(0).toLatin1());
            Q_EMIT overlaysChanged(url, getOverlays(url));
        });
    }

    void setRoots(const QDBusMessage &reply)
    {
        if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().isEmpty()) return;
        roots = reply.arguments().at(0).toStringList();
        known.clear();
    }

    bool inFolder(const QString &path) const
    {
        for (const QString &root : roots) {
            if (path == root || path.startsWith(root + QLatin1Char('/'))) return true;
        }
        return false;
    }

private Q_SLOTS:
    void loadRoots()
    {
        auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(serviceCall(QStringLiteral("FolderRoots"))), this);
        connect(watcher, &QDBusPendingCallWatcher::finished, this, [this](QDBusPendingCallWatcher *done) {
            setRoots(done->reply());
            done->deleteLater();
        });
    }

    void statesChanged(const QStringList &paths)
    {
        for (const QString &path : paths) {
            known.remove(path);
            const QUrl url = QUrl::fromLocalFile(path);
            Q_EMIT overlaysChanged(url, getOverlays(url));
        }
    }
};

#include "gdriveoverlay.moc"
