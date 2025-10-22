# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  pkgs,
  crane,
}:
let
  craneLib = crane.mkLib pkgs;

  # Common arguments can be set here to avoid repeating them later
  # Note: changes here will rebuild all dependency crates
  commonArgs = {
    src = ./.;

    strictDeps = true;
  };

  nw-packet-forwarder = craneLib.buildPackage (
    commonArgs
    // {
      outputs = [ "out" ];
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    }
  );
in
nw-packet-forwarder
