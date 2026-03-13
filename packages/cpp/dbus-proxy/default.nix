# Copyright 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  stdenv,
  cmake,
  pkg-config,
  glib,
  gdk-pixbuf,
  gtk4,
  util-linux,
  cppcheck,
  valgrind,
  dbus,
  python3,
  gobject-introspection,
  lib,
}:
stdenv.mkDerivation {
  pname = "dbus-proxy";
  version = "0.1.2";

  src = ./dbus-proxy;

  nativeBuildInputs = [
    cmake
    pkg-config
    cppcheck
    valgrind
    dbus
    (python3.withPackages (ps: [ ps.pygobject3 ]))
    gobject-introspection
  ];
  buildInputs = [
    glib
    gdk-pixbuf
    gtk4
    util-linux
  ];

  doCheck = true;
  checkPhase = ''
    bash ../tests/sni.sh ./dbus-proxy
  '';

  meta = {
    description = "Cross-bus D-Bus proxy for Ghaf framework";
    homepage = "https://github.com/tiiuae/ghafpkgs";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
    mainProgram = "dbus-proxy";
  };
}
