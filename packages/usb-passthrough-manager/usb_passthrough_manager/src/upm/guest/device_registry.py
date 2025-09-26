# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import json
import os
import stat
import tempfile
from pathlib import Path
from typing import Any

from vsock_bridge.vsock import VsockClient
from vhotplug_schemas import usb_passthrough as gui_schema
from upm.guest.notify_user import show_new_device_popup_async, DeviceStruct
from upm.logger import logger, log_entry_exit


class DeviceRegister:
    def __init__(self, cid: int, port: int, data_dir: str):
        self.server = VsockClient(
            on_message=self.on_msg,
            on_connect=self.on_connect,
            on_disconnect=self.on_disconnect,
            cid=cid,
            port=port,
        )

        self.connected = False
        self.device_registry = {}
        self.regpath = Path(data_dir)
        self.regpath.mkdir(parents=True, exist_ok=True)
        try:
            # Ensure directory is accessible by other users to read the registry file.
            os.chmod(self.regpath, 0o755)
        except OSError as e:
            logger.error(
                f"Failed to set permissions on registry directory {self.regpath}: {e}"
            )

        self.regFile = self.regpath / "usb_db.json"
        if not self.regFile.exists():
            self.regFile.write_text("{}", encoding="utf-8")

        # Set permissions to be readable by all users.
        # This service runs as root, so we need to explicitly set permissions.
        try:
            os.chmod(self.regFile, 0o644)
        except OSError as e:
            logger.error(
                f"Failed to set permissions on registry file {self.regFile}: {e}"
            )

    def __del__(self):
        self.stop()
        if self.regFile.exists():
            os.remove(self.regFile)

    @log_entry_exit
    def start(self):
        logger.debug("")
        self.server.start()

    @log_entry_exit
    def stop(self):
        self.server.stop()

    @log_entry_exit
    def wait(self):
        self.server.join()

    @log_entry_exit
    def request_passthrough(self, device_id: str, new_vm: str) -> bool:
        passthrough_request = gui_schema.PassthroughRequest(device_id, new_vm)
        if not self.server.send(passthrough_request.to_json()):
            logger.error("Failed to send passthrough request to host")
            return False
        return True

    @log_entry_exit
    def on_connect(self):
        self.connected = True
        logger.info("Connected to Host, Requesting devices...")
        get_devices_request = gui_schema.GetDevices()
        if not self.server.send(get_devices_request.to_json()):
            logger.critical("System error! Service restart required.")

    @log_entry_exit
    def on_disconnect(self):
        if self.connected:
            logger.info("Host Disconnected;")
        self.connected = False

    @log_entry_exit
    def atomic_write_registry(self, data: dict[str, Any]) -> None:
        tmp_name = None
        try:
            # Atomic write: write to tmp then replace
            with tempfile.NamedTemporaryFile(
                "w", delete=False, dir=self.regpath, encoding="utf-8"
            ) as tf:
                json.dump(data, tf, indent=2, ensure_ascii=False)
                tmp_name = tf.name
                os.fchmod(
                    tf.fileno(),
                    stat.S_IRUSR | stat.S_IWUSR | stat.S_IRGRP | stat.S_IROTH,
                )  # 0644
            os.replace(tmp_name, self.regFile)  # atomic on POSIX
        except Exception as e:
            logger.error(f"Failed to write registry: {e}")
        finally:
            if tmp_name and os.path.exists(tmp_name):
                try:
                    os.unlink(tmp_name)
                except OSError as e:
                    logger.error(f"Failed to clean up temporary file: {tmp_name}: {e}")

    @log_entry_exit
    def on_msg(self, msg: dict[str, Any]):
        msgtype = gui_schema.get_message_type(msg)
        # A new device connected
        logger.info(f"New message received: {msgtype}")
        if msgtype == gui_schema.DeviceConnected.MSG_TYPE:
            dev = gui_schema.DeviceConnected.from_message(msg)

            self.device_registry[dev.device_id] = dev.device
            self.atomic_write_registry(self.device_registry)
            logger.info(f"device_connected: {dev.device_id} -> {dev.current_vm}")
            dev_struct = DeviceStruct(
                passthrough_handler=self.request_passthrough,
                device_id=dev.device_id,
                vendor=dev.vendor,
                product=dev.product,
                permitted_vms=dev.permitted_vms,
                current_vm=dev.current_vm,
            )

            show_new_device_popup_async(dev_struct=dev_struct)
        # A device removed
        elif msgtype == gui_schema.DeviceRemoved.MSG_TYPE:
            dev = gui_schema.DeviceRemoved.from_message(msg)
            if dev.device_id in self.device_registry:
                del self.device_registry[dev.device_id]
                self.atomic_write_registry(self.device_registry)
                logger.info(f"Device: {dev.device_id} removed")
            else:
                logger.error(f"{dev.device_id} not found!")
        # A snapshot of connected devices
        elif msgtype == gui_schema.ConnectedDevices.MSG_TYPE:
            connected_devices = gui_schema.ConnectedDevices.from_message(msg)
            self.device_registry = connected_devices.devices
            self.atomic_write_registry(self.device_registry)
        # A device switched
        elif msgtype == gui_schema.PassthroughAck.MSG_TYPE:
            ack = gui_schema.PassthroughAck.from_message(msg)
            if ack.status == "ok":
                self.device_registry[ack.device_id]["current_vm"] = ack.current_vm
                self.atomic_write_registry(self.device_registry)
            else:
                logger.error(f"Passthrough failed: {ack.device_id} -> {ack.current_vm}")
        elif msgtype == gui_schema.Reset.MSG_TYPE:
            self.device_registry.clear()
            self.atomic_write_registry(self.device_registry)
        else:
            logger.error(f"Unexpected msg: {msg}")
