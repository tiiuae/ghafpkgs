# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
(
  _final: prev:
  let
    inherit (prev) callPackage;
  in
  {
    ghaf-artwork = callPackage ./ghaf-artwork { };
    ghaf-theme = callPackage ./ghaf-theme { };
  }
)
