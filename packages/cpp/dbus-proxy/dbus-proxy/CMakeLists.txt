#  Copyright 2026 TII (SSRC) and the Ghaf contributors
#  SPDX-License-Identifier: Apache-2.0

cmake_minimum_required(VERSION 3.10)
project(dbus-proxy CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Debug build settings# Different flags for different build types
set(CMAKE_C_FLAGS_DEBUG "-g -O0 -DDEBUG -Wall -Wextra")
set(CMAKE_C_FLAGS_RELEASE "-O2 -DNDEBUG")
set(CMAKE_C_FLAGS_RELWITHDEBINFO "-O2 -g -DNDEBUG")
set(CMAKE_C_FLAGS_MINSIZEREL "-Os -DNDEBUG")

# For C++ (if needed)
set(CMAKE_CXX_FLAGS_DEBUG "${CMAKE_C_FLAGS_DEBUG}")
set(CMAKE_CXX_FLAGS_RELEASE "${CMAKE_C_FLAGS_RELEASE}")

# Use pkg-config to find GLib/GIO
find_package(PkgConfig REQUIRED)
pkg_check_modules(GLIB2 REQUIRED glib-2.0 gio-2.0)

include_directories(${GLIB2_INCLUDE_DIRS})
link_directories(${GLIB2_LIBRARY_DIRS})
add_definitions(${GLIB2_CFLAGS_OTHER})

set(SRCS
  dbus-proxy.cpp
  logging.cpp
  handlers.cpp
  proxy_core.cpp
  callback-rules.cpp
  callback-handling.cpp
  dbus_proxy.h
)

add_executable(dbus-proxy ${SRCS})

target_compile_options(dbus-proxy PRIVATE -Wall -Wextra -g)

target_link_libraries(dbus-proxy ${GLIB2_LDFLAGS})
