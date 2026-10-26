# O submenu "Google Drive" do botão direito no Dolphin (e em quem usa os menus do KIO): um
# plugin KFileItemAction que só desenha o que o serviço responde pelo D-Bus.
cmake_minimum_required(VERSION 3.22)
project(gdrive-desktop-linux-dolphin LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(ECM 6.0 REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})
include(KDEInstallDirs)
include(KDECMakeSettings)
include(KDECompilerSettings NO_POLICY_SCOPE)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core DBus Gui Widgets)
find_package(KF6 6.0 REQUIRED COMPONENTS CoreAddons KIO)

kcoreaddons_add_plugin(gdrive_desktop_linux_menu
    SOURCES gdrivemenu.cpp
    INSTALL_NAMESPACE "kf6/kfileitemaction")
target_link_libraries(gdrive_desktop_linux_menu
    Qt6::DBus Qt6::Gui Qt6::Widgets KF6::CoreAddons KF6::KIOWidgets)

# Os pacotes compilam com -DBUILD_TESTING=OFF.
if(BUILD_TESTING)
    add_executable(gdl-dolphin-probe tests/probe.cpp)
    target_link_libraries(gdl-dolphin-probe Qt6::Widgets KF6::CoreAddons KF6::KIOCore KF6::KIOWidgets)
    add_test(NAME plugin COMMAND ${CMAKE_CURRENT_SOURCE_DIR}/tests/run.sh ${CMAKE_BINARY_DIR})
endif()
