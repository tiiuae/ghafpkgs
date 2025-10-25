# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import argparse
from ghaf_usb_applet.notification_handler import USBDeviceNotification
from ghaf_usb_applet.logger import setup_logger


def build_parser():
    p = argparse.ArgumentParser(description="USB Device notifier")
    p.add_argument(
        "--port", type=int, default=2000, help="Host vsock listen port (default 7000)"
    )
    p.add_argument("--loglevel", type=str, default="info", help="Log level")
    return p


def main():
    args = build_parser().parse_args()
    setup_logger(args.loglevel)
    USBDeviceNotification(server_port=args.port)


if __name__ == "__main__":
    main()
