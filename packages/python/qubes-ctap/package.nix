# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  yubico,
  fido2,
  buildPythonPackage,
  packaging,
  setuptools,
  fetchFromGitHub,
}:
buildPythonPackage {
  pname = "qubes-ctap";
  version = "2.0.6";
  pyproject = true;

  dependencies = [
    yubico
    fido2
    packaging
    setuptools
  ];
  src = fetchFromGitHub {
    owner = "QubesOS";
    repo = "qubes-app-u2f";
    rev = "8a8866b48ba59ed81f52081e38a38484855f39a3";
    sha256 = "sha256-9j/0yfaYQqBKROKDjphPOOvvWL1Qf1LuCW79ubT9mts=";
  };
  patches = [
    ./0001-fix-ghaf-Allow-changing-qrexec-path-from-cli.patch
  ];
  meta = {
    description = "U2F token proxy app";
    platforms = [
      "aarch64-linux"
      "x86_64-linux"
    ];
  };
  postInstall = ''
    mkdir $out/bin
    cp $out/lib/python*/site-packages/usr/bin/* $out/bin/
  '';
}
