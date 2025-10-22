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

    buildInputs = with pkgs; [
      openssl
    ];

    nativeBuildInputs = with pkgs; [
      pkg-config
    ];
  };

  nw-packet-forwarder = craneLib.buildPackage (
    commonArgs
    // {
      pname = "nw-pckt-fwd";
      version = "0.1.0";

      outputs = [ "out" ];
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      meta = with pkgs.lib; {
        description = "Network packet forwarder for Ghaf";
        license = licenses.asl20;
        platforms = platforms.linux;
      };
    }
  );
in
nw-packet-forwarder
