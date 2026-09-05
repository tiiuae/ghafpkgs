# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ callPackage, crane }:
let
  imageTools = callPackage ./ghaf-image-tools { inherit crane; };
in
{
  ghaf-image-tools = imageTools;
  ghaf-initialize-verity-lvm = imageTools.lvm // {
    meta = imageTools.meta // {
      mainProgram = "ghaf-initialize-verity-lvm";
    };
  };
  ghaf-wrap-luks-image = imageTools.luks // {
    meta = imageTools.meta // {
      mainProgram = "ghaf-wrap-luks-image";
    };
  };
  ghaf-kill-switch-app = callPackage ./ghaf-kill-switch-app { inherit crane; };
  ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit crane; };
  ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit crane; };
}
