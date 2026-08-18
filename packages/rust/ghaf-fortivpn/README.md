<!--
SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Ghaf Fortinet VPN

This package creates native NetworkManager FortiSSLVPN profiles from a COSMIC interface in
`gui-vm`. A small privileged service in `net-vm` validates the request, installs optional
certificate material, and creates the system connection through NetworkManager's D-Bus API. The
new profile then appears in COSMIC **Network & Wireless → VPN** for normal connection management.

The form accepts:

- a connection name, gateway, port, optional realm, username, and VPN password;
- an optional trusted-gateway SHA-256 certificate fingerprint;
- an optional PKCS#12 bundle (`.p12` or `.pfx`) and import password;
- alternatively, a PEM or DER X.509 client certificate with a PEM, DER, or PKCS#8 private key;
- an optional PEM or DER gateway CA certificate.

The GUI uses libcosmic and follows the active COSMIC light or dark theme. Client certificates are
not required when the gateway supports password-only authentication.

## Security model

- The VPN account password crosses only Ghaf's authenticated D-Bus transport and NetworkManager's
  local system bus. It is never placed in a command line, environment variable, log, or
  application state file.
- NetworkManager stores the account password in its root-owned system connection. This is required
  for activation from COSMIC in the split `gui-vm`/`net-vm` design, where no NetworkManager secret
  agent runs beside the VPN backend.
- PKCS#12 passwords and private-key passphrases are used only for import and are not persisted.
  Secret buffers owned by the GUI and backend are zeroized after use. D-Bus and NetworkManager may
  make internal transient copies while serializing a request.
- Certificate inputs have strict size limits and are parsed in the backend with OpenSSL. The
  backend verifies certificate validity and that a client certificate matches its private key.
- Imported private keys are normalized to unencrypted PKCS#8 and stored as root-owned mode `0600`
  files below `/var/lib/ghaf/fortivpn`. The enclosing directories are mode `0700`; public
  certificates carry the read bits required by the FortiSSLVPN plugin.
- The backend validates profile fields and creates the connection directly over NetworkManager's
  D-Bus API. Secrets are never passed to `nmcli`.

The consuming NixOS configuration must sandbox the backend and restrict its system-bus interface
to the Ghaf cross-VM proxy identity.

## Build

```sh
nix build .#ghaf-fortivpn
nix build .#ghaf-fortivpn-service
```
