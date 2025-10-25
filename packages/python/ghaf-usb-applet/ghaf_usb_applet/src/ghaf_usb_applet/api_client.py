# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import socket
import json
import threading
import time

from ghaf_usb_applet.logger import logger


class APIClient:
    def __init__(self, port=2000, cid=2):
        self.port = port
        self.cid = cid
        self.sock = None

    def connect(self):
        logger.info("Connecting to vsock cid %s on port %s", self.cid, self.port)
        self.sock = socket.socket(socket.AF_VSOCK, socket.SOCK_STREAM)
        self.sock.connect((self.cid, self.port))
        logger.info("Connected")

    def send(self, msg):
        data = json.dumps(msg) + "\n"
        self.sock.sendall(data.encode("utf-8"))
        return self.recv()

    def recv(self):
        buffer = ""
        while True:
            data = self.sock.recv(4096)
            if not data:
                logger.info("API connection closed by remote")
                break
            buffer += data.decode("utf-8")
            while "\n" in buffer:
                msg, buffer = buffer.split("\n", 1)
                try:
                    return json.loads(msg)
                except ValueError:
                    logger.error("Invalid JSON in API response: %s", msg)
        return None

    def close(self):
        if self.sock:
            self.sock.close()

    def enable_notifications(self):
        response = self.send({"action": "enable_notifications"})
        if response.get("result") != "ok":
            logger.error("Failed to enable notifications: %s", response)

    def usb_list(self):
        return self.send({"action": "usb_list"})

    def usb_attach(self, device_node, vm):
        return self.send({"action": "usb_attach", "device_node": device_node, "vm": vm})

    def usb_detach(self, device_node):
        return self.send({"action": "usb_detach", "device_node": device_node})

    # pylint: disable=too-many-positional-arguments
    @classmethod
    def recv_notifications(cls, callback, port=2000, cid=2, reconnect_delay=3):
        client = cls(port=port, cid=cid)

        def _listener():
            while True:
                try:
                    client.connect()
                    client.enable_notifications()

                    buffer = ""
                    while True:
                        data = client.sock.recv(4096)
                        if not data:
                            raise ConnectionError(
                                "API connection for notifications closed by remote"
                            )
                        buffer += data.decode("utf-8")
                        while "\n" in buffer:
                            msg, buffer = buffer.split("\n", 1)
                            try:
                                parsed = json.loads(msg)
                                callback(parsed)
                            except ValueError:
                                logger.error(
                                    "Invalid JSON in API notification: %s", msg
                                )
                except OSError as e:
                    logger.warning("Notification listener error: %s", e)
                    logger.warning("Reconnecting in %s sec...", reconnect_delay)
                finally:
                    client.close()
                    time.sleep(reconnect_delay)

        thread = threading.Thread(target=_listener, daemon=True)
        thread.start()
        return thread, client

    def get_devices_pretty(self):
        devices = self.usb_list()
        logger.debug(f"{devices}")
        device_map = {}
        unique_idx = 1
        if devices.get("result") == "ok":
            for dev in devices.get("usb_devices", []):
                logger.debug(dev)
                allowed_vms = dev.get("allowed_vms", None)
                if allowed_vms is None or len(allowed_vms) == 0:
                    continue
                if len(allowed_vms) > 1:
                    if "None" not in allowed_vms and "none" not in allowed_vms:
                        allowed_vms.insert(0, "None")
                vm = dev.get("vm", None)
                if vm is None:
                    dev["vm"] = "None"
                if "device_node" in dev and "product_name" in dev:
                    product_name = dev.get("product_name")
                    if product_name is None:
                        continue

                    if product_name.isdigit():
                        product_name = "<unknown device>"
                    product_name = product_name.replace("_", " ")
                    if product_name not in device_map:
                        device_map[product_name] = dev
                    else:
                        product_name = product_name + "(" + str(unique_idx) + ")"
                        device_map[product_name] = dev
                        unique_idx += 1
        return device_map
