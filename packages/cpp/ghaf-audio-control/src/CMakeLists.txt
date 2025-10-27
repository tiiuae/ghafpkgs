# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

cmake_minimum_required(VERSION 3.23)

project(GhafAudioControl
    VERSION 1.0.0
    LANGUAGES CXX
    DESCRIPTION "Ghaf Audio Control Application"
)

# Modern CMake standards
set(CMAKE_CXX_STANDARD 23)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Compiler flags - use modern approach
add_compile_options(-Wall -Wextra -fPIE)

# Modern output paths
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/bin)
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY ${CMAKE_BINARY_DIR}/lib)

# Development tools - generate compile_commands.json for language servers
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

# Ensure compile_commands.json is copied to source directory for better IDE support
if(CMAKE_EXPORT_COMPILE_COMMANDS)
    set(CMAKE_CXX_STANDARD_INCLUDE_DIRECTORIES ${CMAKE_CXX_IMPLICIT_INCLUDE_DIRECTORIES})
endif()

# Development options (disabled by default)
# option(ENABLE_ASAN "Enable AddressSanitizer" OFF)
# if(ENABLE_ASAN)
#     add_compile_options(-fsanitize=address -fsanitize-recover=all)
#     add_link_options(-fsanitize=address -fsanitize-recover=all)
# endif()

# Include GNUInstallDirs for standard installation paths
include(GNUInstallDirs)

# Build subdirectories
add_subdirectory(lib)
add_subdirectory(app)

# Copy compile_commands.json to source root for better IDE/language server support
if(CMAKE_EXPORT_COMPILE_COMMANDS)
    add_custom_target(copy-compile-commands ALL
        DEPENDS ${CMAKE_BINARY_DIR}/compile_commands.json
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            ${CMAKE_BINARY_DIR}/compile_commands.json
            ${CMAKE_SOURCE_DIR}/compile_commands.json
        COMMENT "Copying compile_commands.json to source directory for IDE support"
    )
endif()
