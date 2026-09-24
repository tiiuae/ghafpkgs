# SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
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
    pname = "ghaf-kill-switch";
    version = "0.1.0";

    # Links the libraries libcosmic dlopen()s and wraps the binary with the
    # COSMIC icon and default-theme data dirs, as nixpkgs COSMIC apps do.
    nativeBuildInputs = with pkgs; [
      pkg-config
      libcosmicAppHook
    ];

    # Environment variables for build
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "1";

    # Pin the tree hash of every git dependency in Cargo.lock. Without these,
    # crane resolves each one with `builtins.fetchGit { allRefs = true; }` at
    # evaluation time, which mirrors every ref the remote advertises -- GitHub
    # serves refs/pull/* -- and prints the whole fetch to the eval log. Given a
    # hash, crane uses a plain fetchgit derivation instead, so the checkout is
    # substitutable and evaluation stays offline.
    # Regenerate after any cargo update that moves a revision: crane reports the
    # expected value as a hash mismatch.
    outputHashes = {
      "git+https://github.com/iced-rs/cryoglyph.git?rev=e429a025df36ab8145708acb309080ae3deec17a#e429a025df36ab8145708acb309080ae3deec17a" =
        "sha256-10JUHl1ktbqLaReuiU3HPa4r2KvsoryyJoF3BFoge3U=";
      "git+https://github.com/jackpot51/rust-atomicwrites#043ab4859d53ffd3d55334685303d8df39c9f768" =
        "sha256-QZSuGPrJXh+svMeFWqAXoqZQxLq/WfIiamqvjJNVhxA=";
      "git+https://github.com/pop-os/cosmic-panel#f416dbbe72d8600d395d24bf9f96f6ba1299d13b" =
        "sha256-3nIlsngBTH7WZGVGjSeVvJUY3rWvORnfby0TfgiRWLg=";
      "git+https://github.com/pop-os/cosmic-protocols?rev=32283d7#32283d76a8d0342da74c4cc022a533c52dcf378f" =
        "sha256-LUAmB+3+doRZOJbVURaIInaQuV/LXCKfoWHA28ihAMo=";
      "git+https://github.com/pop-os/dbus-settings-bindings#eed01dd3609e90e3c8cd043656734c500956c793" =
        "sha256-LYIR+qK+hCBVV+bfVWz2jvH5fGvfNTcryKqfe5n8Gog=";
      "git+https://github.com/pop-os/freedesktop-icons#ab4c57b8e416c6af9297cb04d101889896fd9a92" =
        "sha256-tPriTi5L0mFMHjo5xpF5cmKGHqlX3WUO7EZAgVdBpS4=";
      "git+https://github.com/pop-os/libcosmic#1f6dc991eaa5115a09be2505cc8a323e5b2b0bff" =
        "sha256-sPOXJiukAdOvRJ5cet0ec+0LwGeoLBtkmZLSzBzKcnA=";
      "git+https://github.com/pop-os/smithay-clipboard?tag=sctk-0.20#859b02c88f45c554049a67c6ddeec1692ce0e20b" =
        "sha256-GojAFRbhJcP0Rpr+v9WOivgW9x38PZdeBWTbMhkDB3A=";
      "git+https://github.com/pop-os/softbuffer?tag=cosmic-4.0#c2b2c19ddb38ff17495643699f97cb1f2064a1be" =
        "sha256-9Ret/nfieBFl4yJ9TddyWsSuS7sI4QAza/TZrxYMb+I=";
      "git+https://github.com/pop-os/window_clipboard.git?tag=sctk-0.20#f68595ee0e62fbd6589f4709b5aaa5c3c7ea5f6c" =
        "sha256-WO3JFbE+6ESRAfkxrnEFeZyGuhUHLOKOVHcGQyHwoK0=";
      "git+https://github.com/pop-os/winit.git?tag=cosmic-0.14#71ce08c043814514a8fd92d9d0599f115ae854e8" =
        "sha256-8r9O5RgVa8vxkPPYvr2aQiRdZ4isg7Jdnk8O5gQIr9k=";
      "git+https://github.com/wash2/accesskit?tag=cosmic-0.14#f0599eed5f18111228266fe3f28991cc48b5964f" =
        "sha256-pP9CyiV1zIONQ7vbl5MkMtilemSPrHaZ0c/SyR+lb0k=";
    };
  };

  # Build only the cargo dependencies (for caching)
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Run cargo test
  cargoTest = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

  # Run cargo clippy for linting
  cargoClippy = craneLib.cargoClippy (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoClippyExtraArgs = "--all-targets -- --deny warnings";
    }
  );

  # Build the actual application
  ghaf-kill-switch = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;

      passthru.tests = {
        inherit cargoTest cargoClippy;
      };

      postInstall = ''
        install -Dm644 data/ae.tii.CosmicAppletKillSwitch.desktop -t $out/share/applications
      '';

      # Metadata for the final package
      meta = {
        description = "Kill Switch app for Ghaf virtualization platform";
        longDescription = ''
          COSMIC panel applet that blocks the microphone, camera, Wi-Fi and
          Bluetooth by detaching them from their VMs through the device
          manager (vhotplug) API.
        '';
        homepage = "https://ghaf.dev";
        license = lib.licenses.asl20;
        platforms = [
          "x86_64-linux"
          "aarch64-linux"
        ];
        mainProgram = "cosmic-applet-kill-switch";
      };
    }
  );
in
ghaf-kill-switch
