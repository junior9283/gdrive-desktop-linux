// Os emblemas de estado nos arquivos do Meu Drive no Dolphin: na nuvem, off-line, sincronizando
// ou em conflito. O estado vem do atributo `user.gdl.state` que a montagem dá a cada item (fora
// dela não há o atributo, e nada aparece); quando ele muda, o serviço manda os caminhos no sinal
// `StatesChanged` e o emblema é redesenhado.

#include <KOverlayIconPlugin>

#include <QDBusConnection>
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
        QDBusConnection::sessionBus().connect(SERVICE, PATH, INTERFACE, QStringLiteral("StatesChanged"), this,
                                              SLOT(statesChanged(QStringList)));
    }

    // A leitura do atributo vem do banco local do serviço (sem rede), por isso cabe aqui.
    QStringList getOverlays(const QUrl &url) override
    {
        if (!url.isLocalFile()) return {};
        const QByteArray path = QFile::encodeName(url.toLocalFile());
        char value[16];
        const ssize_t size = getxattr(path.constData(), "user.gdl.state", value, sizeof value);
        const QByteArray state(value, qMax<ssize_t>(size, 0));
        if (!STATES.contains(state)) return {};
        return {QStringLiteral("gdl-emblem-") + QString::fromLatin1(state)};
    }

private Q_SLOTS:
    void statesChanged(const QStringList &paths)
    {
        for (const QString &path : paths) {
            const QUrl url = QUrl::fromLocalFile(path);
            Q_EMIT overlaysChanged(url, getOverlays(url));
        }
    }
};

#include "gdriveoverlay.moc"
