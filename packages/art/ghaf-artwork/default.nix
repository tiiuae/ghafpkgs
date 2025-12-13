# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ lib, stdenvNoCC, ... }:
stdenvNoCC.mkDerivation {
  pname = "ghaf-artwork";
  src = ./.;
  version = "0.1.0";
  meta = {
    description = "Ghaf Artwork";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
  };
  installPhase = ''
    mkdir -p $out
    cp -r * $out
    rm $out/default.nix
  '';
}
