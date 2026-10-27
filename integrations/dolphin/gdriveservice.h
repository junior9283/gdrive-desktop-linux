// O endereço D-Bus do serviço, o mesmo para os plugins do menu e dos emblemas.

#pragma once

#include <QDBusMessage>
#include <QString>
#include <QVariantList>

namespace gdrive
{
inline const QString SERVICE = QStringLiteral("io.github.junior9283.GDriveDesktopLinux1");
inline const QString PATH = QStringLiteral("/io/github/junior9283/GDriveDesktopLinux1");
inline const QString INTERFACE = QStringLiteral("io.github.junior9283.GDriveDesktopLinux1.Service");

// Uma chamada ao serviço que não o inicia: parado, ele só não responde (e nada aparece).
inline QDBusMessage serviceCall(const QString &method, const QVariantList &args = {})
{
    auto call = QDBusMessage::createMethodCall(SERVICE, PATH, INTERFACE, method);
    call.setArguments(args);
    call.setAutoStartService(false);
    return call;
}
}
