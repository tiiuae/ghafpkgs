# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

# Find dependencies
find_package(PkgConfig REQUIRED)
pkg_check_modules(GTKMM REQUIRED gtkmm-3.0)
find_package(PulseAudio REQUIRED)

# Define library
set(LIBRARY_NAME GhafAudioControl)
add_library(${LIBRARY_NAME})

# Source files
target_sources(${LIBRARY_NAME}
    PRIVATE
        src/Backends/PulseAudio/AudioControlBackend.cpp
        src/Backends/PulseAudio/GeneralDevide.cpp
        src/Backends/PulseAudio/Helpers.cpp
        src/Backends/PulseAudio/Sink.cpp
        src/Backends/PulseAudio/SinkInput.cpp
        src/Backends/PulseAudio/Source.cpp
        src/Backends/PulseAudio/SourceOutput.cpp
        src/Backends/PulseAudio/Volume.cpp
        src/models/DeviceListModel.cpp
        src/models/DeviceModel.cpp
        src/utils/ConnectionContainer.cpp
        src/utils/Debug.cpp
        src/utils/Logger.cpp
        src/widgets/AppList.cpp
        src/widgets/AudioControl.cpp
        src/widgets/DeviceListWidget.cpp
        src/widgets/DeviceWidget.cpp
        src/widgets/SinkWidget.cpp
        src/AppVmMetaGroup.cpp
        src/MetaDevice.cpp
)

# Public headers using modern FILE_SET (CMake 3.23+) for better IDE/export behavior
target_sources(${LIBRARY_NAME}
    PUBLIC
        FILE_SET public_headers
        TYPE HEADERS
        BASE_DIRS include
        FILES
            # Backend headers
            include/GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp
            include/GhafAudioControl/Backends/PulseAudio/GeneralDevice.hpp
            include/GhafAudioControl/Backends/PulseAudio/Helpers.hpp
            include/GhafAudioControl/Backends/PulseAudio/Sink.hpp
            include/GhafAudioControl/Backends/PulseAudio/SinkInput.hpp
            include/GhafAudioControl/Backends/PulseAudio/Source.hpp
            include/GhafAudioControl/Backends/PulseAudio/SourceOutput.hpp
            include/GhafAudioControl/Backends/PulseAudio/Volume.hpp
            
            # Model headers
            include/GhafAudioControl/models/DeviceListModel.hpp
            include/GhafAudioControl/models/DeviceModel.hpp
            
            # Utility headers
            include/GhafAudioControl/utils/Check.hpp
            include/GhafAudioControl/utils/ConnectionContainer.hpp
            include/GhafAudioControl/utils/Debug.hpp
            include/GhafAudioControl/utils/Logger.hpp
            include/GhafAudioControl/utils/RaiiWrap.hpp
            include/GhafAudioControl/utils/ScopeExit.hpp
            
            # Widget headers
            include/GhafAudioControl/widgets/AppList.hpp
            include/GhafAudioControl/widgets/AudioControl.hpp
            include/GhafAudioControl/widgets/DeviceListWidget.hpp
            include/GhafAudioControl/widgets/DeviceWidget.hpp
            include/GhafAudioControl/widgets/SinkWidget.hpp
            
            # Core headers
            include/GhafAudioControl/AppVmMetaGroup.hpp
            include/GhafAudioControl/IAudioControlBackend.hpp
            include/GhafAudioControl/MetaDevice.hpp
            include/GhafAudioControl/Volume.hpp
)

# Link libraries using modern target-based approach
target_link_libraries(${LIBRARY_NAME}
    PUBLIC
        ${GTKMM_LIBRARIES}
    PRIVATE
        ${PULSEAUDIO_LIBRARY}
        ${PULSEAUDIO_MAINLOOP_LIBRARY}
)

# Include directories using modern generator expressions
target_include_directories(${LIBRARY_NAME}
    PUBLIC
        ${GTKMM_INCLUDE_DIRS}
        "$<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>"
        "$<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>"
    PRIVATE
        ${PulseAudio_INCLUDE_DIRS}
)

# Link directories (still needed for some older libraries)
target_link_directories(${LIBRARY_NAME}
    PUBLIC
        ${GTKMM_LIBRARY_DIRS}
)

# Installation
install(
    TARGETS ${LIBRARY_NAME}
    EXPORT GhafAudioControlTargets
    LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
    ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR}
    RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
    FILE_SET public_headers
)

# Export targets for other CMake projects
install(
    EXPORT GhafAudioControlTargets
    FILE GhafAudioControlTargets.cmake
    NAMESPACE GhafAudioControl::
    DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/GhafAudioControl
)
