# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.16)
project(dbus-proxy CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(PkgConfig REQUIRED)
pkg_check_modules(GIO      REQUIRED IMPORTED_TARGET gio-2.0)
pkg_check_modules(GLIB     REQUIRED IMPORTED_TARGET glib-2.0)
pkg_check_modules(GDK_PIXBUF REQUIRED IMPORTED_TARGET gdk-pixbuf-2.0)
pkg_check_modules(GTK4     REQUIRED IMPORTED_TARGET gtk4)

add_executable(dbus-proxy
    dbus-proxy.cpp
    sni/proxy.cpp
)

target_compile_options(dbus-proxy PRIVATE -Wall -Wextra -g)

target_link_libraries(dbus-proxy PRIVATE
    PkgConfig::GIO
    PkgConfig::GLIB
    PkgConfig::GDK_PIXBUF
    PkgConfig::GTK4
)

install(TARGETS dbus-proxy RUNTIME DESTINATION bin)
