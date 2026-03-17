# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ callPackage, crane }:
{
  ghaf-kill-switch-app = callPackage ./ghaf-kill-switch-app { inherit crane; };
  ghaf-mem-manager = callPackage ./ghaf-mem-manager { inherit crane; };
  ghaf-nw-packet-forwarder = callPackage ./ghaf-nw-packet-forwarder { inherit crane; };
  ghaf-virtiofs-tools = callPackage ./ghaf-virtiofs-tools { inherit crane; };
}
