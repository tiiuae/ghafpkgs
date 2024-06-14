# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ lib, runCommand, ... }:
runCommand "ghaf-theme"
  {
    version = "0.1.0";
    meta = with lib; {
      description = "Ghaf Theme";
      longDescription = ''
        The Ghaf theme is currently intended for labwc (Wayland compositor), and
        to be used with the Ghaf platform.
      '';
      license = licenses.asl20;
      platforms = platforms.linux;
    };
  }
  ''
    mkdir -p $out/share/themes/Ghaf/openbox-3
    cp ${./assets}/* $out/share/themes/Ghaf/openbox-3
    cp ${./themerc}  $out/share/themes/Ghaf/openbox-3/themerc
  ''
