# SPDX-FileCopyrightText: 2025-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  pkgs,
  crane,
}:
let
  craneLib = crane.mkLib pkgs;

  # libraries that may be dlopen()'d at runtime by winit/iced/wgpu, etc.
  dlopenLibraries = with pkgs; [
    libxkbcommon # input handling
    wayland # wayland client lib
    vulkan-loader # vulkan ICD loader
  ];

  # Common arguments can be set here to avoid repeating them later
  # Note: changes here will rebuild all dependency crates
  commonArgs = {
    src = ./.;
    strictDeps = true;

    # Add metadata from Cargo.toml
    pname = "ghaf-kill-switch-app";
    version = "0.1.0";

    nativeBuildInputs = with pkgs; [
      pkg-config
      makeWrapper # we will use this to wrap the installed binary
    ];

    # Environment variables for build
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "1";

    # Include dlopen libs so they are present at build time / available to patchelf if needed
    buildInputs = dlopenLibraries;

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
      "git+https://github.com/pop-os/cosmic-panel#3c08c30c2d77afb5130bbc42a74f479979b372dd" =
        "sha256-dx+k+A5ZXo9MXuUxjdEd4xEqscuaNdVoojQzCWUNy/g=";
      "git+https://github.com/pop-os/cosmic-protocols?rev=32283d7#32283d76a8d0342da74c4cc022a533c52dcf378f" =
        "sha256-LUAmB+3+doRZOJbVURaIInaQuV/LXCKfoWHA28ihAMo=";
      "git+https://github.com/pop-os/dbus-settings-bindings#eed01dd3609e90e3c8cd043656734c500956c793" =
        "sha256-LYIR+qK+hCBVV+bfVWz2jvH5fGvfNTcryKqfe5n8Gog=";
      "git+https://github.com/pop-os/freedesktop-icons#ab4c57b8e416c6af9297cb04d101889896fd9a92" =
        "sha256-tPriTi5L0mFMHjo5xpF5cmKGHqlX3WUO7EZAgVdBpS4=";
      "git+https://github.com/pop-os/libcosmic#c003a58fff660f5c55ebfb54a5908722fa7d1d6e" =
        "sha256-43r6smzg4zIZn/HarkZkchgnXGNnrIvOaf49I9CauOA=";
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
  ghaf-kill-switch-app = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;

      passthru.tests = {
        inherit cargoTest cargoClippy;
      };

      # After install, make a wrapper that ensures LD_LIBRARY_PATH contains
      # the library search path for our dlopen-able libraries.
      postInstall = ''
        if [ -x "$out/bin/ghaf-kill-switch-app" ]; then
          mv "$out/bin/ghaf-kill-switch-app" "$out/bin/cosmic-applet-killswitch"
          wrapProgram "$out/bin/cosmic-applet-killswitch" \
            --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath dlopenLibraries}
        fi
        mkdir -p $out/share/applications
        cat > $out/share/applications/ae.tii.CosmicAppletKillSwitch.desktop <<EOF
        [Desktop Entry]
        Type=Application
        Exec=cosmic-applet-killswitch
        Categories=COSMIC;
        Name=Kill Switch
        Comment=Privacy control applet for microphone, camera and WiFi
        Icon=security-high-symbolic
        StartupNotify=true
        Terminal=false
        NoDisplay=true
        X-CosmicApplet=true
        X-CosmicHoverPopup=Auto
        EOF
      '';

      # Metadata for the final package
      meta = {
        description = "Kill Switch app for Ghaf virtualization platform";
        longDescription = ''
          A simple graphical user interface (GUI) application built using Iced
          library in Rust. It implements a "Kill Switch" functionality allowing
          users to enable or disable their microphone, camera
          and WiFi via toggler controls.
        '';
        homepage = "https://ghaf.dev";
        license = lib.licenses.asl20;
        platforms = lib.platforms.linux;
        mainProgram = "ghaf-kill-switch-app";
      };
    }
  );
in
ghaf-kill-switch-app
