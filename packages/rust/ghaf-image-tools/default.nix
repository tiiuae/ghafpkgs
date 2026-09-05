# SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  pkgs,
  callPackage,
  crane,
  makeBinaryWrapper,
  btrfs-progs,
  util-linux,
  zstd,
  qemu-utils,
  cryptsetup,
}:
let
  craneLib = crane.mkLib pkgs;
  commonArgs = {
    src = craneLib.cleanCargoSource ./.;
    strictDeps = true;
    pname = "ghaf-image-tools";
    version = "0.1.0";
  };
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
  imageTools = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      outputs = [
        "out"
        "lvm"
        "luks"
      ];
      nativeBuildInputs = [ makeBinaryWrapper ];
      # Let Crane strip vendored-source references before splitting outputs.
      preFixup = ''
        mkdir -p "$lvm/bin" "$luks/bin"
        mv "$out/bin/ghaf-initialize-verity-lvm" "$lvm/bin/"
        mv "$out/bin/ghaf-wrap-luks-image" "$luks/bin/"
        wrapProgram "$lvm/bin/ghaf-initialize-verity-lvm" \
          --prefix PATH : "${
            lib.makeBinPath [
              btrfs-progs
              util-linux
              zstd
            ]
          }"
        wrapProgram "$luks/bin/ghaf-wrap-luks-image" \
          --prefix PATH : "${
            lib.makeBinPath [
              qemu-utils
              cryptsetup
            ]
          }"
        ln -s "$lvm/bin/ghaf-initialize-verity-lvm" "$out/bin/"
        ln -s "$luks/bin/ghaf-wrap-luks-image" "$out/bin/"
      '';
      passthru.tests =
        callPackage ./tests.nix {
          inherit imageTools;
        }
        // {
          clippy = craneLib.cargoClippy (
            commonArgs
            // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            }
          );
        };
      meta = {
        description = "Regular-file LVM and LUKS image builders for Ghaf";
        license = lib.licenses.asl20;
        platforms = lib.platforms.linux;
      };
    }
  );
in
imageTools
