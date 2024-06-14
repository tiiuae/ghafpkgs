# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ lib, runCommand, ... }:
runCommand "ghaf-artwork"
  {
    version = "0.1.0";
    meta = with lib; {
      description = "Ghaf Artwork";
      license = licenses.asl20;
      platforms = platforms.linux;
    };
  }
  ''
    mkdir -p $out
    cp -r ${./.} $out
  ''
