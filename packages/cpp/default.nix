# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ callPackage }:
{
  dbus-proxy = callPackage ./dbus-proxy { };
  ghaf-audio-control = callPackage ./ghaf-audio-control { };
  vsockproxy = callPackage ./vsockproxy { };
}
