# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
_: {
  perSystem =
    { pkgs, ... }:
    let
      inherit (pkgs) callPackage;
    in
    {
      packages = {
        ghaf-artwork = callPackage ./ghaf-artwork { };
        ghaf-theme = callPackage ./ghaf-theme { };
      };
    };
}
