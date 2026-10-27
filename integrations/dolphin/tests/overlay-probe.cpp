// Carrega o plugin de emblemas como o Dolphin (pelo QT_PLUGIN_PATH, em kf6/overlayicon) e imprime
// os emblemas de cada caminho; com `--wait N`, espera N avisos de mudança (`overlaysChanged`) e
// imprime cada um. Roda sem tela, contra o serviço falso de `fake_service.py`.
//
// Uso: overlay-probe [--wait N] CAMINHO...

#include <KOverlayIconPlugin>
#include <KPluginMetaData>

#include <QCoreApplication>
#include <QPluginLoader>
#include <QTimer>
#include <QUrl>

#include <cstdio>

static void out(const QString &line)
{
    std::printf("%s\n", qPrintable(line));
    std::fflush(stdout);
}

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    QStringList args = app.arguments().mid(1);
    int wait = 0;
    if (args.value(0) == QLatin1String("--wait")) {
        args.takeFirst();
        wait = args.takeFirst().toInt();
    }
    const auto found = KPluginMetaData::findPlugins(QStringLiteral("kf6/overlayicon"), [](const KPluginMetaData &data) {
        return data.fileName().contains(QLatin1String("gdrive_desktop_linux_overlay"));
    });
    KOverlayIconPlugin *plugin = nullptr;
    if (!found.isEmpty()) {
        QPluginLoader loader(found.first().fileName());
        plugin = qobject_cast<KOverlayIconPlugin *>(loader.instance());
    }
    if (!plugin) {
        out(QStringLiteral("(sem plugin)"));
        return 1;
    }
    for (const QString &path : args) {
        out(path.section(QLatin1Char('/'), -1) + QStringLiteral(": ") + plugin->getOverlays(QUrl::fromLocalFile(path)).join(QLatin1Char(' ')));
    }
    if (wait == 0) return 0;
    QObject::connect(plugin, &KOverlayIconPlugin::overlaysChanged, [&](const QUrl &url, const QStringList &overlays) {
        out(QStringLiteral("mudou ") + url.fileName() + QStringLiteral(": ") + overlays.join(QLatin1Char(' ')));
        if (--wait == 0) app.quit();
    });
    QTimer::singleShot(5000, &app, [&] {
        out(QStringLiteral("(sem aviso)"));
        app.exit(1);
    });
    // Pronto para receber: quem roda o teste já pode pedir o sinal.
    out(QStringLiteral("esperando"));
    return app.exec();
}
