# SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  component,
  crane,
  lib,
  pkgs,
}:
let
  craneLib = crane.mkLib pkgs;
  isGui = component == "gui";
  packageName = if isGui then "ghaf-fortivpn" else "ghaf-fortivpn-service";
  dlopenLibraries = with pkgs; [
    libxkbcommon
    vulkan-loader
    wayland
  ];
  commonArgs = {
    src = if isGui then ./gui else ./service;
    strictDeps = true;
    pname = packageName;
    version = "0.1.0";
    cargoExtraArgs = "-p ${packageName}";
    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "1";
    nativeBuildInputs = [ pkgs.pkg-config ] ++ lib.optional isGui pkgs.makeWrapper;
    buildInputs = lib.optionals isGui dlopenLibraries ++ lib.optional (!isGui) pkgs.openssl;
  };
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
  cargoTest = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });
  cargoClippy = craneLib.cargoClippy (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoClippyExtraArgs = "--all-targets -- --deny warnings";
    }
  );
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;

    passthru.tests = {
      inherit cargoClippy cargoTest;
    };

    postInstall = lib.optionalString isGui ''
      wrapProgram "$out/bin/ghaf-fortivpn" \
        --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath dlopenLibraries}

      install -Dm644 org.ghaf.FortiVpn.desktop \
        "$out/share/applications/org.ghaf.FortiVpn.desktop"
    '';

    meta = {
      description =
        if isGui then
          "Secure Fortinet VPN profile creator for Ghaf"
        else
          "Backend for secure Fortinet VPN profile creation in Ghaf";
      homepage = "https://github.com/tiiuae/ghafpkgs";
      license = lib.licenses.asl20;
      mainProgram = packageName;
      platforms = lib.platforms.linux;
    };
  }
)
