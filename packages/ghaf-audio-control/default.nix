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

  cmakeFlags = [ "" ];

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

  meta = with lib; {
    description = "Ghaf Audio Control Panel";
    platforms = platforms.unix;
  };
}
