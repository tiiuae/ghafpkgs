# Copyright 2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
(_final: prev: {
  ghaf-artwork = import ./ghaf-artwork { inherit prev; };
  ghaf-theme = import ./ghaf-theme { inherit prev; };
})
