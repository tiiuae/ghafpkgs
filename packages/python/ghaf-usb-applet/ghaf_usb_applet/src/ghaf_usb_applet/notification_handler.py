# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import subprocess

from ghaf_usb_applet.api_client import (
    DEFAULT_CID,
    DEFAULT_PORT,
    APIClient,
    format_product_name,
)
from ghaf_usb_applet.logger import logger


class USBDeviceNotification:
    def __init__(self, server_port=DEFAULT_PORT):
        self.port = server_port
        self.callback = None

    def monitor(self, callback):
        th, apiclient = APIClient.recv_notifications(
            callback=self.notify_user,
            port=self.port,
            cid=DEFAULT_CID,
            reconnect_delay=3,
        )
        self.apiclient = apiclient
        self.callback = callback
        return th

    def notify_user(self, msg):
        logger.debug("Device notification: %s", msg)
        event = msg.get("event", "")
        if event == "usb_select_vm":
            self.show_notif_window(msg)
        else:
            self.callback()

    def show_notif_window(self, msg):
        dev = msg.get("usb_device", {})
        allowed = msg.get("allowed_vms", [])
        if len(allowed) < 2:
            logger.error("Not enough VMs available to prompt for a choice")
            return
        dev["allowed_vms"] = allowed
        name = format_product_name(dev.get("product_name"), max_len=20)
        dev["product_name"] = name

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
        except OSError as e:
            logger.error(f"Failed to launch 'usb_device' popup menu, Error: {e}")
