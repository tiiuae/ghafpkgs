<!--
SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->
# QEMU Remote SWTPM Shim

This is a simple shim/proxy that allows QEMU to communicate with a remote instance of [swtpm](https://github.com/stefanberger/swtpm).

## Why?

QEMU supports TPM emulation through the TCG PC Client TPM Interface Specification (TIS) using a local UNIX socket. It is commonly paired with [swtpm](https://github.com/stefanberger/swtpm), which requires two communication channels: a **control channel**, used by QEMU to manage TPM lifecycle events (e.g., VM start/stop), and a **data channel**, which carries TPM commands and responses.

QEMU uses **ancillary data** (`SCM_RIGHTS`) on the control channel to pass a file descriptor for the data channel to `swtpm`. However, this design breaks when the control channel is transported over protocols that do not support such mechanism passing, such as TCP/IP. This shim bridges that gap, enabling remote `swtpm` instances to be used with unmodified QEMU.

## Related Work

In December 2024, a patch series ([tpm: add mssim backend](https://patchwork.kernel.org/project/qemu-devel/cover/20241212170528.30364-1-James.Bottomley@HansenPartnership.com/)) was submitted to QEMU, adding support for the Microsoft TPM simulator, which uses a TCP-based protocol.

While this patch series can be applied with some modifications to recent QEMU versions, its abandonment highlights the need for alternative solutions like the QEMU Remote SWTPM Shim.
