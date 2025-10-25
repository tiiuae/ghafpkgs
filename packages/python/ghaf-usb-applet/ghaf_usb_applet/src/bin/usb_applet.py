# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

from ghaf_usb_applet.applet import start_usb_applet
from ghaf_usb_applet.logger import setup_logger
import argparse


def main():
    parser = argparse.ArgumentParser(description="USB Device Applet")
    parser.add_argument("--loglevel", type=str, default="info", help="Log level")
    parser.add_argument("--port", type=int, default=2000, help="vHotplug server port")
    args = parser.parse_args()
    setup_logger(args.loglevel)
    start_usb_applet(args.port)
