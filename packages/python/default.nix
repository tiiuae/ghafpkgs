# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ python3Packages }:
{
  ldap-query = python3Packages.callPackage ./ldap-query/package.nix { };
  vinotify = python3Packages.callPackage ./vinotify/package.nix { };
  ghaf-usb-applet = python3Packages.callPackage ./ghaf-usb-applet/package.nix { };
  gps-websock = python3Packages.callPackage ./gps-websock/package.nix { };
  qubes-ctap = python3Packages.callPackage ./qubes-ctap/package.nix { };
}
