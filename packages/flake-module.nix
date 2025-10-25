# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ inputs, ... }:
let
  ghafpkgs =
    pkgs:
    let
      inherit (pkgs) callPackage lib;

      # Filter function to remove override attributes from package sets
      filterPackages =
        packageSet:
        lib.filterAttrs (
          name: _value:
          !(lib.elem name [
            "override"
            "overrideDerivation"
          ])
        ) packageSet;

      artPackages = filterPackages (callPackage ./art { inherit pkgs; });
      pythonPackages = filterPackages (callPackage ./python { inherit pkgs; });
      goPackages = filterPackages (callPackage ./go { inherit pkgs; });
      rustPackages = filterPackages (
        callPackage ./rust {
          inherit pkgs;
          inherit (inputs) crane;
        }
      );
      cppPackages = filterPackages (callPackage ./cpp { inherit pkgs; });

      # Utility packages
      utilityPackages = {
        update-deps = callPackage ./update-deps { };
      };
    in
    artPackages // pythonPackages // goPackages // rustPackages // cppPackages // utilityPackages;
in
{
  perSystem =
    { pkgs, ... }:
    {
      packages = ghafpkgs pkgs;
    };

  flake.overlays.default = _final: prev: {
    # Art packages
    ghaf-artwork = prev.callPackage ./art/ghaf-artwork { };
    ghaf-theme = prev.callPackage ./art/ghaf-theme { };
    ghaf-wallpapers = prev.callPackage ./art/ghaf-wallpapers { };

    # Python packages
    ghaf-usb-applet = prev.python3Packages.callPackage ./python/ghaf-usb-applet/package.nix { };
    gps-websock = prev.python3Packages.callPackage ./python/gps-websock/package.nix { };
    hotplug = prev.python3Packages.callPackage ./python/hotplug/package.nix { };
    ldap-query = prev.python3Packages.callPackage ./python/ldap-query/package.nix { };
    vhotplug = prev.python3Packages.callPackage ./python/vhotplug/package.nix { };
    vinotify = prev.python3Packages.callPackage ./python/vinotify/package.nix { };

    # Rust packages (these are actually in rust directory)
    ghaf-mem-manager = prev.callPackage ./rust/ghaf-mem-manager { inherit (inputs) crane; };
    ghaf-nw-packet-forwarder = prev.callPackage ./rust/ghaf-nw-packet-forwarder {
      inherit (inputs) crane;
    };

    # Go packages (this is actually in go directory)
    swtpm-proxy-shim = prev.callPackage ./go/swtpm-proxy-shim { };

    # C++ packages
    ghaf-audio-control = prev.callPackage ./cpp/ghaf-audio-control { };
    vsockproxy = prev.callPackage ./cpp/vsockproxy { };

    # Utility packages
    update-deps = prev.callPackage ./update-deps { };
  };
}
