# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  description = "Rust development dev shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      crane,
      rust-overlay,
      advisory-db,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        inherit (pkgs) lib;

        craneLib = crane.mkLib pkgs;
        src = craneLib.cleanCargoSource ./.;
        commonArgs = {
          inherit src;
          strictDeps = true;

          buildInputs = with pkgs; [
            openssl
            pkg-config
            eza
            fd
            clang
            #cargo-audit
            cargo-tarpaulin
          ];
        };

        individualCrateArgs = commonArgs // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml { inherit src; }) version;
          doNotPostBuildInstallCargoBinaries = true;

          doCheck = false;
          cargoVendorDir = craneLib.vendorMultipleCargoDeps {
            inherit (craneLib.findCargoFiles src) cargoConfigs;
            cargoLockList = [ ./Cargo.lock ];
          };
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        fileSetForCrate =
          crate:
          lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              ./Cargo.toml
              ./Cargo.lock
              ./src
              crate
            ];
          };

        # Sequential flake checking can be utilized for CI/CD purposes.
        # Run squence cmd: 'nix flake check'
        # 1. Check formatting
        nwPcktFwdPackage-cargoFmt = craneLib.cargoFmt (
          individualCrateArgs // { inherit src cargoArtifacts; }
        );

        #  2. Run clippy (and deny all warnings) on the crate source.
        nwPcktFwdPackage-cargoClippy = craneLib.cargoClippy (
          individualCrateArgs
          // {
            # Again we apply some extra arguments only to this derivation
            # and not every where else. In this case we add some clippy flags
            cargoArtifacts = nwPcktFwdPackage-cargoFmt;
            nativeBuildInputs = with pkgs; [ ];
            preBuild = ''
              cargo build --release
            '';
            cargoClippyExtraArgs = "-- --deny warnings";
          }
        );

        # 3. we want to run the tests and collect code-coverage, _but only if
        # the clippy checks pass_ so we do not waste any extra cycles.
        nwPcktFwdPackage-cargoTarpaulin = craneLib.cargoTarpaulin (
          individualCrateArgs // { cargoArtifacts = nwPcktFwdPackage-cargoClippy; }
        );

        # 4. cargo-audit
        nwPcktFwdPackage-cargoAudit = craneLib.cargoAudit (
          individualCrateArgs
          // {
            inherit advisory-db;
            cargoArtifacts = nwPcktFwdPackage-cargoTarpaulin;
          }
        );

        mknwPcktFwdPackage =
          buildType:
          craneLib.buildPackage (
            individualCrateArgs
            // {
              pname = "nw-pckt-fwd";
              cargoExtraArgs = "";
              src = fileSetForCrate ./.;
              #CARGO_BUILD_RUSTFLAGS = "-C link-arg=-lasan -Zproc-macro-backtrace";
              nativeBuildInputs = with pkgs; [
                openssl
                pkg-config
                eza
                fd
                clang
                cargo-audit
              ];
              buildPhaseCargoCommand = ''
                if [[ "${buildType}" == "release" ]]; then
                     cargo build --release
                  else
                     cargo build
                  fi

              '';

              installPhase = ''
                mkdir -p $out/bin
                install -D -m755 target/${buildType}/nw-pckt-fwd $out/bin/${buildType}/nw-pckt-fwd
              '';
            }
          );
        # Create packages for different build types
        nwPcktFwdRelease = mknwPcktFwdPackage "release";
        nwPcktFwdDebug = mknwPcktFwdPackage "debug";
      in
      with pkgs;
      {
        formatter = pkgs.alejandra;
        packages = {
          inherit nwPcktFwdRelease nwPcktFwdDebug;
          default = nwPcktFwdRelease; # Default to release build
        };
        checks = {
          inherit
            # Build the crate as part of `nix flake check` for convenience
            nwPcktFwdRelease
            nwPcktFwdPackage-cargoAudit
            ;
          # Run:
          # nix build .#checks.x86_64-linux.integrationTest
          # To run interactively:
          # nix build .#checks.x86_64-linux.integrationTest.driverInteractive
          # See: https://nixos.org/manual/nixos/stable/index.html#sec-running-nixos-tests-interactively
          # ./result/bin/nixos-test-driver
          integrationTest = pkgs.nixosTest (
            import ./test/integration/packet_gen_tests.nix {
              inherit pkgs;
              inherit crane;
            }
          );
        };
        devShells.default = craneLib.devShell {
          # Inherit inputs from checks.
          checks = self.checks.${system};
          inherit (commonArgs) buildInputs;
        };
      }
    );
}
