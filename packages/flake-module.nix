# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  # Build all ghafpkgs packages given a pkgs set
  # This function is reused by both perSystem.packages and the overlay
  #
  # Uses `import` instead of `callPackage` for category directories so that
  # the resulting attrset has statically-known attribute names.  This keeps the
  # overlay lazy: nixpkgs's fixed-point can determine which names the overlay
  # contributes without forcing `pkgs.callPackage` (which would cause infinite
  # recursion when the overlay is composed with other overlays via
  # `composeManyExtensions`).
  mkGhafpkgs =
    {
      pkgs,
      crane,
      ghafFortivpnSrc,
    }:
    let
      inherit (pkgs) callPackage python3Packages;

      artPackages = import ./art { inherit callPackage; };
      pythonPackages = import ./python { inherit python3Packages; };
      goPackages = import ./go { inherit callPackage; };
      inherit (import ./qemu { inherit callPackage; }) x86QemuPackages nvidiaQemuPackages;
      rustPackages = import ./rust {
        inherit callPackage;
        inherit crane;
        inherit ghafFortivpnSrc;
      };
      cppPackages = import ./cpp { inherit callPackage; };

      utilityPackages = {
        ghaf-networkmanager-openconnect =
          (pkgs.networkmanager-openconnect.override { withGnome = false; }).overrideAttrs
            (old: {
              patches = old.patches ++ [ (ghafFortivpnSrc + "/integrations/fortivpn-openconnect.patch") ];
            });
        update-deps = callPackage ./update-deps { };
      };
    in
    artPackages
    // pythonPackages
    // goPackages
    // x86QemuPackages
    // nvidiaQemuPackages
    // rustPackages
    // cppPackages
    // utilityPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = mkGhafpkgs {
        inherit pkgs;
        inherit (inputs) crane;
        ghafFortivpnSrc = inputs.ghaf-fortivpn;
      };
    };

  flake.overlays.fortivpn-desktop = _final: prev: {
    cosmic-settings-daemon = prev.cosmic-settings-daemon.overrideAttrs (old: {
      postPatch = (old.postPatch or "") + ''
        cp ${inputs.ghaf-fortivpn}/integrations/cosmic-settings-daemon.rs src/fortivpn.rs
        substituteInPlace src/main.rs \
          --replace-fail 'mod greeter;' 'mod greeter; mod fortivpn;' \
          --replace-fail 'let (varlink_daemon, varlink_backend)' 'task::spawn_local(fortivpn::watch()); let (varlink_daemon, varlink_backend)'
      '';
    });
  };

  # Overlay for use by downstream consumers
  # Provides all ghafpkgs packages when applied to a nixpkgs set
  flake.overlays.default =
    _final: prev:
    mkGhafpkgs {
      pkgs = prev;
      inherit (inputs) crane;
      ghafFortivpnSrc = inputs.ghaf-fortivpn;
    };
}
