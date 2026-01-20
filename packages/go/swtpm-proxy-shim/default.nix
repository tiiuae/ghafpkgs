# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  buildGoModule,
}:
buildGoModule {
  pname = "swtpm-proxy";
  version = "0.0.1";
  src = ./.;

  # To update vendorHash after go.mod changes:
  # 1. Set to lib.fakeHash
  # 2. Run: nix build .#swtpm-proxy-shim
  # 3. Copy the correct hash from the error message
  vendorHash = "sha256-xzEp2O9byt0qivp0hHmlJ6Dtwn6pNfgD1uquI72burc=";

  subPackages = [ "cmd/swtpm-proxy" ];

  overrideModAttrs = _old: {
    buildFlags = [ "-mod=mod" ];
  };

  meta = {
    description = "A proxy for swtpm written in Go.";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}
