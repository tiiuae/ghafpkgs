# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  stdenv,
  cmake,
  gtkmm3,
  libayatana-appindicator,
  libpulseaudio,
  ninja,
  pkg-config,
}:

stdenv.mkDerivation rec {
  pname = "ghaf-audio-control";
  version = "1.0.0";

  src = ./src;

  nativeBuildInputs = [
    cmake
    ninja
    pkg-config
  ];

  buildInputs = [
    gtkmm3
    libayatana-appindicator
    libpulseaudio
  ];

  # CMake will automatically find the minimum required version (3.23)
  # and configure the build appropriately
  cmakeFlags = [
    # Explicit build type for consistency
    "-DCMAKE_BUILD_TYPE=Release"
    # Ensure C++23 standard is used (matching CMakeLists.txt)
    "-DCMAKE_CXX_STANDARD=23"
    "-DCMAKE_CXX_STANDARD_REQUIRED=ON"
    "-DCMAKE_CXX_EXTENSIONS=OFF"
  ];

  # Ensure proper installation paths
  postInstall = ''
    # Verify that the expected binaries are present
    test -f $out/bin/GhafAudioControlStandalone
    test -f $out/lib/libGhafAudioControl.a
  '';

  meta = with lib; {
    description = "Ghaf Audio Control Panel";
    longDescription = ''
      A GTK-based audio control panel for the Ghaf platform that provides
      audio device management through PulseAudio backend. Features include
      volume control, device switching, and D-Bus interface for system
      integration.
    '';
    homepage = "https://github.com/tiiuae/ghafpkgs";
    license = licenses.asl20;
    maintainers = with maintainers; [ ];
    platforms = platforms.linux;
    mainProgram = "GhafAudioControlStandalone";
  };
}
