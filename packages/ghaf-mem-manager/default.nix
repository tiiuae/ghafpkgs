# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  pkgs,
  crane,
  src,
}:
let
  craneLib = crane.mkLib pkgs;

  # Common arguments can be set here to avoid repeating them later
  # Note: changes here will rebuild all dependency crates
  commonArgs = {
    src = ./.;

    strictDeps = true;
  };

  mem-monitor = craneLib.buildPackage (
    commonArgs
    // {
      outputs = [ "out" ];
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

    }
  );
in
mem-monitor
