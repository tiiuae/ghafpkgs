# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import argparse

from ghaf_usb_applet.api_client import DEFAULT_PORT
from ghaf_usb_applet.logger import setup_logger
from ghaf_usb_applet.notification_handler import USBDeviceNotification


def build_parser():
    p = argparse.ArgumentParser(description="USB Device notifier")
    p.add_argument(
        "--port",
        type=int,
        default=DEFAULT_PORT,
        help=f"vhotplug server port (default: {DEFAULT_PORT})",
    )
    p.add_argument("--loglevel", type=str, default="info", help="Log level")
    return p


def main():
    args = build_parser().parse_args()
    setup_logger(args.loglevel)
    notif = USBDeviceNotification(server_port=args.port)
    thread = notif.monitor(lambda: None)
    thread.join()


if __name__ == "__main__":
    main()
