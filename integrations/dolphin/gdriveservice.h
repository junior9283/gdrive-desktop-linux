// O endereço D-Bus do serviço, o mesmo para os plugins do menu e dos emblemas.

#pragma once

#include <QString>

namespace gdrive
{
inline const QString SERVICE = QStringLiteral("io.github.junior9283.GDriveDesktopLinux1");
inline const QString PATH = QStringLiteral("/io/github/junior9283/GDriveDesktopLinux1");
inline const QString INTERFACE = QStringLiteral("io.github.junior9283.GDriveDesktopLinux1.Service");
}
