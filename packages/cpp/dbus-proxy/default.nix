# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  stdenv,
  pkg-config,
  glib,
  lib,
}:
stdenv.mkDerivation {
  pname = "dbus-proxy";
  version = "0.1.0";

  src = ./dbus-proxy;

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ glib ];

  installPhase = ''
    mkdir -p $out/bin
    install -Dm755 dbus-proxy $out/bin/dbus-proxy
  '';

  meta = {
    description = "Cross-bus D-Bus proxy for Ghaf framework";
    homepage = "https://github.com/tiiuae/ghafpkgs";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
    mainProgram = "dbus-proxy";
  };
}
