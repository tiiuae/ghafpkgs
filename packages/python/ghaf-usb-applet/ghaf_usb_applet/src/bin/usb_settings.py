# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import argparse

from ghaf_usb_applet.api_client import DEFAULT_PORT
from ghaf_usb_applet.settings import SettingsMenu


def main():
    parser = argparse.ArgumentParser(description="USB Passthrough Settings")
    parser.add_argument("--loglevel", type=str, default="info", help="Log level")
    parser.add_argument(
        "--port",
        type=int,
        default=DEFAULT_PORT,
        help=f"vhotplug server port (default: {DEFAULT_PORT})",
    )
    args = parser.parse_args()

    from ghaf_usb_applet.logger import setup_logger

    setup_logger(args.loglevel)

    app = SettingsMenu(port=args.port)
    app.run()


if __name__ == "__main__":
    main()
