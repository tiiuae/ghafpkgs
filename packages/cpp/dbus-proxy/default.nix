# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  stdenv,
  pkgs,
  lib,
  ...
}:
stdenv.mkDerivation {
  name = "dbus-proxy";

  src = ./dbus-proxy;

  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.glib ];

  sourceRoot = "./dbus-proxy";

  installPhase = ''
    mkdir -p $out/bin
    install -Dm755 dbus-proxy $out/bin/dbus-proxy
  '';
  meta = {
    description = "DBus proxy";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    license = lib.licenses.asl20;
    mainProgram = "dbus-proxy";
  };
}
