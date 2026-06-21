# SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  pkgs,
  crane,
}:
let
  craneLib = crane.mkLib pkgs;

  # Common arguments
  commonArgs = {
    pname = "ghaf-virtiofs-tools";
    version = "0.1.0";
    src = ./.;
    strictDeps = true;

    nativeBuildInputs = with pkgs; [
      pkg-config
      makeWrapper
      clippy
    ];

    buildInputs = with pkgs; [
      clamav
      coreutils
    ];

    # Environment variables for build
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "1";
  };

  # Build only the cargo dependencies (for caching)
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Build the application with clippy integrated
  ghaf-virtiofs-tools = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;

      # Integrate clippy into the build process with fail-on-warnings
      buildPhaseCargoCommand = ''
        echo "Running clippy with fail-on-warnings..."
        cargo clippy --all-targets --all-features --release -- -W clippy::all -W clippy::pedantic -W clippy::nursery -D warnings

        echo "Building package..."
        cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)
        cargo build --release --message-format json-render-diagnostics >"$cargoBuildLog"
      '';

      # Metadata for the final package
      meta = {
        description = "Virtiofs tools for secure cross-VM file sharing";
        longDescription = ''
          Secure file sharing between VMs using virtiofs in the Ghaf framework.
          Includes virtiofs-gate (host daemon), clamd-vclient (guest scanner),
          clamd-vproxy (ClamAV proxy), and virtiofs-notify (change notifications).
        '';
        homepage = "https://github.com/tiiuae/ghaf";
        license = lib.licenses.asl20;
        maintainers = [ ];
        platforms = lib.platforms.linux;
        mainProgram = "virtiofs-gate";
      };
    }
  );
in
ghaf-virtiofs-tools
