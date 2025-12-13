# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
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
    pname = "ghaf-mem-manager";
    version = "0.1.0";

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
  ghaf-mem-manager = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;

      # Metadata for the final package
      meta = {
        description = "Memory management utilities for Ghaf virtualization platform";
        longDescription = ''
          A memory management service for the Ghaf framework that provides
          memory monitoring, allocation tracking, and resource management
          for virtualized environments. Features include memory usage monitoring,
          QEMU integration, and resource optimization.
        '';
        homepage = "https://ghaf.dev";
        license = lib.licenses.asl20;
        platforms = lib.platforms.linux;
        mainProgram = "ghaf-mem-manager";
      };
    }
  );
in
ghaf-mem-manager
