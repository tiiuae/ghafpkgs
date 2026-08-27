# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import argparse

from ghaf_usb_applet.api_client import DEFAULT_PORT
from ghaf_usb_applet.applet import start_usb_applet
from ghaf_usb_applet.logger import setup_logger


def main():
    parser = argparse.ArgumentParser(description="USB Device Applet")
    parser.add_argument("--loglevel", type=str, default="info", help="Log level")
    parser.add_argument(
        "--port",
        type=int,
        default=DEFAULT_PORT,
        help=f"vhotplug server port (default: {DEFAULT_PORT})",
    )
    args = parser.parse_args()
    setup_logger(args.loglevel)
    start_usb_applet(args.port)
