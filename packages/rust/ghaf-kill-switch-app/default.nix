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
  };

  # Build only the cargo dependencies (for caching)
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Build the actual application
  ghaf-kill-switch-app = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
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
