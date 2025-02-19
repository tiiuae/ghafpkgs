# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

cmake_minimum_required(VERSION 3.5)

project(GhafAudioControlStandalone LANGUAGES CXX)

find_package(PkgConfig REQUIRED)
pkg_check_modules(AYATANA REQUIRED ayatana-appindicator3-0.1)

include_directories(${AYATANA_INCLUDE_DIRS})
link_directories(${AYATANA_INCLUDE_DIRS})

add_executable(GhafAudioControlStandalone

    dbus/Interface.hpp
    dbus/Methods.hpp
    dbus/Service.hpp

    App.hpp


    dbus/Interface.cpp
    dbus/Methods.cpp
    dbus/Service.cpp

    App.cpp
    main.cpp
)
target_link_libraries(GhafAudioControlStandalone GhafAudioControl ${AYATANA_LIBRARIES})

include(GNUInstallDirs)
install(TARGETS ${PROJECT_NAME})
