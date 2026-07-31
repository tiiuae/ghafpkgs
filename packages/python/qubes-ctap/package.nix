# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  yubico,
  fido2,
  buildPythonApplication,
  lib,
  packaging,
  setuptools,
  fetchFromGitHub,
}:

buildPythonApplication {
  pname = "qubes-ctap";
  version = "2.0.6";
  pyproject = true;

  src = fetchFromGitHub {
    owner = "QubesOS";
    repo = "qubes-app-u2f";
    rev = "8a8866b48ba59ed81f52081e38a38484855f39a3";
    sha256 = "sha256-9j/0yfaYQqBKROKDjphPOOvvWL1Qf1LuCW79ubT9mts=";
  };

  dependencies = [
    yubico
    fido2
    packaging
    setuptools
  ];

  patches = [
    ./0001-fix-ghaf-Allow-changing-qrexec-path-from-cli.patch
    # Upstream still calls asyncio.get_event_loop() before a loop exists, which
    # Python 3.14 no longer tolerates. Drop this once upstream creates the loop
    # explicitly.
    ./0002-fix-ghaf-Create-the-event-loop-explicitly-for-Python.patch
  ];

  postInstall = ''
    mkdir -p $out/bin
    cp $out/lib/python*/site-packages/usr/bin/* $out/bin/
  '';

  meta = {
    description = "U2F token proxy app";
    license = lib.licenses.gpl2Plus;
    platforms = [
      "aarch64-linux"
      "x86_64-linux"
    ];
    mainProgram = "qctap-proxy";
  };
}
