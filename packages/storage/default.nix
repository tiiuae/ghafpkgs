# SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ callPackage }:
{
  lvm2-offline = callPackage ./lvm2-offline { };
  cryptsetup-offline = callPackage ./cryptsetup-offline { };
}
