# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ callPackage }:
{
  ghaf-artwork = callPackage ./ghaf-artwork { };
  ghaf-theme = callPackage ./ghaf-theme { };
  ghaf-wallpapers = callPackage ./ghaf-wallpapers { };
}
