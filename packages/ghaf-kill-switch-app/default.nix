# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
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
  commonArgs = rec {
    src = ./.;

    strictDeps = true;

    nativeBuildInputs = with pkgs; [
      pkg-config
      makeWrapper # we will use this to wrap the installed binary
    ];

    # Include dlopen libs so they are present at build time / available to patchelf if needed
    buildInputs = dlopenLibraries;
  };

  killswitch = craneLib.buildPackage (
    commonArgs
    // {
      outputs = [ "out" ];
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
      # After install, make a wrapper that ensures LD_LIBRARY_PATH contains
      # the library search path for our dlopen-able libraries.
      postInstall = ''
        if [ -x "$out/bin/ghaf-kill-switch-app" ]; then
          wrapProgram "$out/bin/ghaf-kill-switch-app" \
            --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath dlopenLibraries}
        fi
        # Copy icons alongside the binary
        mkdir -p $out/icons
        cp -r $src/src/icons/* $out/icons/
      '';
    }
  );
in
killswitch
