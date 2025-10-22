# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ lib, stdenvNoCC, ... }:
stdenvNoCC.mkDerivation {
  pname = "ghaf-artwork";
  src = ./.;
  version = "0.1.0";
  meta = with lib; {
    description = "Ghaf Artwork";
    license = licenses.asl20;
    platforms = platforms.linux;
  };
  installPhase = ''
    mkdir -p $out
    cp -r * $out
    rm $out/default.nix
  '';
}
