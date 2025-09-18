# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import argparse
import logging
import sys

from upm.guest.device_bridge import DeviceBridge
from upm.logger import setup_logger

logger = logging.getLogger("upm")


def build_parser():
    p = argparse.ArgumentParser(description="Guest USB controller")
    p.add_argument("--hostport", type=int, default=7000, help="vsock listen port")
    p.add_argument(
        "--dir",
        type=str,
        default="/tmp/usb-passthrough/",
        help="Database directory for USB device manager, should be same as usb passthrough manager service",
    )
    p.add_argument("--loglevel", type=str, default="info", help="Log level")
    return p


def main():
    args = build_parser().parse_args()
    setup_logger(args.loglevel)

    app = DeviceBridge(data_dir=args.dir)
    sys.exit(app.run(None))


if __name__ == "__main__":
    main()
