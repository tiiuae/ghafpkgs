# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import argparse
from ghaf_usb_applet.logger import setup_logger
from ghaf_usb_applet.vm_selection import show_device_setting


def parse_args():
    parser = argparse.ArgumentParser(description="USB Device Notification")

    parser.add_argument("--title", type=str, default="USB Device", help="Window title")
    parser.add_argument("--loglevel", type=str, default="info", help="Log level")
    parser.add_argument("--port", type=int, default=2000, help="vHotPlug server port")
    parser.add_argument(
        "--device_node",
        type=str,
        required=True,
        help="Device node path, e.g. /dev/bus/usb/001/004",
    )
    parser.add_argument(
        "--product_name", type=str, required=True, help="Product name of the device"
    )
    parser.add_argument(
        "--allowed_vms",
        nargs="+",
        required=True,
        help="List of allowed VMs (space-separated)",
    )
    parser.add_argument("--vm", type=str, default="", help="Currently selected VM")

    return parser.parse_args()


def main():
    args = parse_args()
    setup_logger(args.loglevel)
    device = {
        "device_node": args.device_node,
        "product_name": args.product_name,
        "allowed_vms": args.allowed_vms,
        "vm": args.vm,
    }
    show_device_setting(device=device, title=args.title, port=args.port)


if __name__ == "__main__":
    main()
