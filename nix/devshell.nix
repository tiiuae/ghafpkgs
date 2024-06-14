# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  perSystem =
    { pkgs, ... }:
    {
      devShells.default = pkgs.mkShell {
        name = "Ghaf Artwork devshell";
        packages = with pkgs; [
          bashInteractive
          git
          nix
          alejandra
          reuse
          imagemagick
        ];
      };
    };
}
