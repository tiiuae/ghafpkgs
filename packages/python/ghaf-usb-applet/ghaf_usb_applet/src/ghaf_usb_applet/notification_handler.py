# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import subprocess
import json

from ghaf_usb_applet.api_client import APIClient
from ghaf_usb_applet.logger import logger


def format_product_name(dev):
    product_name = dev.get("product_name", None)
    if product_name is None:
        dev["product_name"] = "<unknown device>"
    else:
        product_name = product_name.replace("_", " ")
        dev["product_name"] = product_name[:20]


class USBDeviceNotification:
    def __init__(self, server_port=2000):
        self.port = server_port
        self.callback = None

    def monitor(self, callback):
        th, apiclient = APIClient.recv_notifications(
            callback=self.notify_user, port=self.port, cid=2, reconnect_delay=3
        )
        self.apiclient = apiclient
        self.callback = callback
        return th

    def notify_user(self, msg):
        logger.info(f"Device notification: {json.dumps(msg, indent=4)}")
        event = msg.get("event", "")
        if event == "usb_select_vm":
            self.show_notif_window(msg)
        else:
            self.callback()

    def show_notif_window(self, msg):
        dev = msg.get("usb_device", {})
        allowed = msg.get("allowed_vms", [])
        if len(allowed) < 2:
            logger.error("VMs not available to make choice")
            return
        dev["allowed_vms"] = allowed
        format_product_name(dev)

        name = dev.get("product_name", "<unknown device>")
        name = name.replace("_", " ")
        cmd = [
            "usb_device",
            "--title",
            "New device attached!",
            "--device_node",
            dev.get("device_node", ""),
            "--product_name",
            name,
            "--allowed_vms",
            *dev.get("allowed_vms", []),
        ]

        selected = dev.get("vm", None)
        if selected:
            cmd = cmd + ["--vm", selected]

        logger.debug(cmd)
        try:
            subprocess.Popen(cmd)
        except Exception as e:
            logger.error(f"Failed to launch 'usb_device' popup menu, Error: {e}")
