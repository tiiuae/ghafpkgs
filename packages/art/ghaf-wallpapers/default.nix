# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ stdenv, lib }:

stdenv.mkDerivation {
  pname = "ghaf-wallpapers";

  version = "0.1.0";

  src = ./wallpapers;

  installPhase = ''
    install -D -m 0644 -t "$out/share/backgrounds/ghaf" $src/*
  '';

  meta = with lib; {
    description = "Wallpaper backgrounds for Ghaf";

    license = {
      fullName = "Unsplash License";
      url = "https://unsplash.com/license";
      free = true;
    };

    platforms = platforms.linux;
  };
}
