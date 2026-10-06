# SPDX-FileCopyrightText: 2022-2026 TII (CRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
cmake_minimum_required(VERSION 3.16)
project(led-proxy C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Debug CACHE STRING "Build type" FORCE)
endif()
find_package(Threads REQUIRED)

set(CMAKE_C_FLAGS_DEBUG "-g -O0 -DDEBUG -Wall -Wextra")
set(CMAKE_C_FLAGS_RELEASE "-O2 -DNDEBUG")
set(CMAKE_C_FLAGS_RELWITHDEBINFO "-O2 -g -DNDEBUG")
set(CMAKE_C_FLAGS_MINSIZEREL "-Os -DNDEBUG")

add_executable(led-proxy-server
    led-proxy-server.c common/common.c
)

add_executable(led-proxy-client
    led-proxy-client.c common/common.c
)

target_include_directories(led-proxy-server PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
)
target_include_directories(led-proxy-client PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
)

target_compile_options(led-proxy-server PRIVATE -Wall -Wextra)
target_compile_options(led-proxy-client PRIVATE -Wall -Wextra)

target_link_libraries(led-proxy-server PRIVATE Threads::Threads)
target_link_libraries(led-proxy-client PRIVATE Threads::Threads)

install(TARGETS led-proxy-server RUNTIME DESTINATION bin)
install(TARGETS led-proxy-client RUNTIME DESTINATION bin)
