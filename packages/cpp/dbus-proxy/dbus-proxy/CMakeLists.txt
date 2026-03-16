# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.16)
project(dbus-proxy CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Different flags for different build types
set(CMAKE_C_FLAGS_DEBUG "-g -O0 -DDEBUG -Wall -Wextra")
set(CMAKE_C_FLAGS_RELEASE "-O2 -DNDEBUG")
set(CMAKE_C_FLAGS_RELWITHDEBINFO "-O2 -g -DNDEBUG")
set(CMAKE_C_FLAGS_MINSIZEREL "-Os -DNDEBUG")

# For C++
set(CMAKE_CXX_FLAGS_DEBUG "${CMAKE_C_FLAGS_DEBUG}")
set(CMAKE_CXX_FLAGS_RELEASE "${CMAKE_C_FLAGS_RELEASE}")

find_package(PkgConfig REQUIRED)
pkg_check_modules(GIO        REQUIRED IMPORTED_TARGET gio-2.0)
pkg_check_modules(GLIB       REQUIRED IMPORTED_TARGET glib-2.0)
pkg_check_modules(GDK_PIXBUF REQUIRED IMPORTED_TARGET gdk-pixbuf-2.0)
pkg_check_modules(GTK4       REQUIRED IMPORTED_TARGET gtk4)
pkg_check_modules(RSVG       REQUIRED IMPORTED_TARGET librsvg-2.0)
pkg_check_modules(CAIRO      REQUIRED IMPORTED_TARGET cairo)

add_executable(dbus-proxy
    dbus-proxy.cpp
    handlers.cpp
    proxy_core.cpp
    callback-rules.cpp
    callback-handling.cpp
    dbus_proxy.h
    sni/proxy.cpp
)

target_include_directories(dbus-proxy PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/sni
)

target_compile_options(dbus-proxy PRIVATE -Wall -Wextra -g)

target_link_libraries(dbus-proxy PRIVATE
    PkgConfig::GIO
    PkgConfig::GLIB
    PkgConfig::GDK_PIXBUF
    PkgConfig::GTK4
    PkgConfig::RSVG
    PkgConfig::CAIRO
)

install(TARGETS dbus-proxy RUNTIME DESTINATION bin)
