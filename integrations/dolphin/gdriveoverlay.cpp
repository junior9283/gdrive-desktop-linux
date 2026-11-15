// Os emblemas de estado nos arquivos do Meu Drive no Dolphin: na nuvem, baixado (no cache), off-line, sincronizando,
// em conflito ou com erro. O estado vem do atributo `user.gdl.state` que a montagem dá a cada
// item; nas pastas do computador sincronizadas (que não têm o atributo), do método `FolderStates` do
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
#include <QPointer>
#include <QSet>
#include <QThreadPool>
#include <QUrl>

#include <sys/xattr.h>

#include "gdriveservice.h"

using namespace gdrive;

namespace
{
// Os valores de `user.gdl.state` (crates/gdl-fs/src/layout.rs); cada um tem o emblema
// `gdl-emblem-<valor>` em assets/icons/hicolor/scalable/emblems.
const QList<QByteArray> STATES = {"cloud", "cached", "offline", "syncing", "conflict", "error"};
// Só ao carregar o plugin a lista de raízes é esperada (o serviço responde do banco local).
constexpr int ROOTS_TIMEOUT_MS = 200;
// Quantos emblemas voltam para a thread principal de uma vez.
constexpr int REDRAW_BATCH = 100;

// O que `user.gdl.state` diz de um caminho; `attribute` é falso fora da montagem (sem o atributo).
struct Read
{
    QString path;
    bool attribute = false;
    QByteArray state;
};

// Bloqueia enquanto o serviço responde (a montagem é dele): nunca na thread principal em lote.
Read readState(const QString &path)
{
    char value[16];
    const ssize_t size = getxattr(QFile::encodeName(path).constData(), "user.gdl.state", value, sizeof value);
    return {path, size >= 0, QByteArray(value, qMax<ssize_t>(size, 0))};
}
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
        const QString local = url.toLocalFile();
        seen.insert(local);
        return overlays(url, readState(local));
    }

private:
    QStringList roots;
    // O estado de cada caminho das pastas, como o serviço respondeu (vazio enquanto pergunta).
    QHash<QString, QByteArray> known;
    // Os caminhos que o Dolphin já pediu (os que ele mostra): só esses precisam de aviso de mudança.
    QSet<QString> seen;

    // Os emblemas de `url` a partir do que o atributo disse (`read`); só na thread principal.
    QStringList overlays(const QUrl &url, const Read &read)
    {
        QByteArray state = read.state;
        if (!read.attribute) {
            if (!inFolder(read.path)) return {};
            const auto found = known.constFind(read.path);
            if (found == known.constEnd()) {
                askState(url);
                return {};
            }
            state = *found;
        }
        if (!STATES.contains(state)) return {};
        return {QStringLiteral("gdl-emblem-") + QString::fromLatin1(state)};
    }

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

    // Lê o atributo dos `paths` fora da thread principal e devolve os emblemas em lotes. Depois de
    // limpar o cache, milhares de caminhos mudam de uma vez; lê-los aqui travava o Dolphin.
    void refresh(const QStringList &paths)
    {
        QPointer<GDriveOverlay> self(this);
        QThreadPool::globalInstance()->start([self, paths] {
            QList<Read> batch;
            const auto redraw = [&self](const QList<Read> &reads) {
                if (!self) return;
                QMetaObject::invokeMethod(self.data(), [self, reads] {
                    if (!self) return;
                    for (const Read &read : reads) {
                        const QUrl url = QUrl::fromLocalFile(read.path);
                        Q_EMIT self->overlaysChanged(url, self->overlays(url, read));
                    }
                }, Qt::QueuedConnection);
            };
            for (const QString &path : paths) {
                batch << readState(path);
                if (batch.size() == REDRAW_BATCH) {
                    redraw(batch);
                    batch.clear();
                }
            }
            if (!batch.isEmpty()) redraw(batch);
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
        // Só os que o Dolphin já pediu; os outros leem o estado novo quando ele pedir.
        QStringList shown;
        for (const QString &path : paths) {
            known.remove(path);
            if (seen.contains(path)) shown << path;
        }
        if (!shown.isEmpty()) refresh(shown);
    }
};

#include "gdriveoverlay.moc"
