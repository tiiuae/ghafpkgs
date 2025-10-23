# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
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

    # Add metadata from Cargo.toml
    pname = "ghaf-nw-packet-forwarder";
    version = "0.1.0";

    buildInputs =
      with pkgs;
      [
        openssl
      ]
      ++ lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
        # Additional Darwin dependencies if needed
      ];

    nativeBuildInputs = with pkgs; [
      pkg-config
    ];

    # Environment variables for build
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "1";
  };

  # Build only the cargo dependencies (for caching)
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Build the actual application
  ghaf-nw-packet-forwarder = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;

      meta = with lib; {
        description = "Network packet forwarder for Ghaf virtualization platform";
        longDescription = ''
          A high-performance network packet forwarding service for the Ghaf framework.
          Provides efficient packet routing, network virtualization support, and
          integration with QEMU network backends for secure virtualized networking.
        '';
        homepage = "https://ghaf.dev";
        license = licenses.asl20;
        platforms = platforms.linux;
        mainProgram = "nw-pckt-fwd";
      };
    }
  );
in
ghaf-nw-packet-forwarder
