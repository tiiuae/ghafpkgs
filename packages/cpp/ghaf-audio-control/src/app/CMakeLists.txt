# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

# Find dependencies
find_package(PkgConfig REQUIRED)
pkg_check_modules(AYATANA REQUIRED ayatana-appindicator3-0.1)

# Define executable
set(EXECUTABLE_NAME GhafAudioControlStandalone)
add_executable(${EXECUTABLE_NAME})

# Source files
target_sources(${EXECUTABLE_NAME}
    PRIVATE
        dbus/Interface.cpp
        dbus/Methods.cpp
        dbus/Service.cpp
        App.cpp
        main.cpp
)

# Public headers using FILE_SET (CMake 3.23+) for better IDE/export behavior
target_sources(${EXECUTABLE_NAME}
    PUBLIC
        FILE_SET app_headers
        TYPE HEADERS
        BASE_DIRS include
        FILES
            include/ghaf-audio-control-app/dbus/Interface.hpp
            include/ghaf-audio-control-app/dbus/Methods.hpp
            include/ghaf-audio-control-app/dbus/Service.hpp
            include/ghaf-audio-control-app/App.hpp
)

# Include directories
target_include_directories(${EXECUTABLE_NAME}
    PRIVATE
        ${AYATANA_INCLUDE_DIRS}
    PUBLIC
        "$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>"
        "$<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>"
)

# Link libraries
target_link_libraries(${EXECUTABLE_NAME}
    PRIVATE
        GhafAudioControl
        ${AYATANA_LIBRARIES}
)

# Link directories
target_link_directories(${EXECUTABLE_NAME}
    PRIVATE
        ${AYATANA_LIBRARY_DIRS}
)

# Installation
install(
    TARGETS ${EXECUTABLE_NAME}
    EXPORT GhafAudioControlAppTargets
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
    FILE_SET app_headers
)

# Export app targets (useful if someone wants to link against the app headers)
install(
    EXPORT GhafAudioControlAppTargets
    FILE GhafAudioControlAppTargets.cmake
    NAMESPACE GhafAudioControlApp::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/GhafAudioControlApp
)
