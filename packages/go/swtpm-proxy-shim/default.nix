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
  vendorHash = "sha256-rQF9XHF6+ApoQlJJ6vJywO5guklK4h4UwWa4oGgadTw=";
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
