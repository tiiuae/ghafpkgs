# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

{
  stdenv,
  meson,
  ninja,
  lib,
}:

stdenv.mkDerivation {
  pname = "vsockproxy";
  version = "0.1.0";

  src = ./.;

  nativeBuildInputs = [
    meson
    ninja
  ];

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin
    install ./vsockproxy $out/bin/vsockproxy

    runHook postInstall
  '';

  meta = {
    description = "VM Sockets proxy for guest-to-guest communication";
    longDescription = ''
      VM Sockets (vsock) is a communication mechanism between guest virtual machines and the host.
      This tool makes it possible to use vsock for guest to guest communication by listening for
      incoming connections on host, connecting to the guest virtual machine and forwarding data
      in both directions.
    '';
    homepage = "https://github.com/tiiuae/ghafpkgs";
    license = lib.licenses.asl20;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "vsockproxy";
  };
}
