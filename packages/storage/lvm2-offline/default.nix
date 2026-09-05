# SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
# Developed against LVM2 2.03.41; recheck the metadata writer on version bumps.
{ lvm2 }:

(lvm2.override {
  enableCmdlib = false;
  enableDmeventd = false;
  udevSupport = false;
}).overrideAttrs
  (old: {
    pname = "lvm2-offline";

    patches = (old.patches or [ ]) ++ [ ./offline-regular-files.patch ];

    configureFlags = (old.configureFlags or [ ]) ++ [ "--disable-ioctl" ];

    meta = (old.meta or { }) // {
      description = "Offline-only LVM2 tools with regular-file PV support";
    };
  })
