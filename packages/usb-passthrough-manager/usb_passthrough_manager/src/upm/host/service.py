# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

from collections.abc import Callable
from threading import Lock
from typing import Any, TypeVar
from vsock_bridge.vsock import VsockClient
from vhotplug_schemas import usb_passthrough as gui_schema

from upm.logger import logger, log_entry_exit

T = TypeVar("T")


class HostService:
    def __init__(
        self,
        cid: int,
        port: int,
        metadata: T | None = None,
        passthrough_handler: Callable[[T, str, str], bool] | None = None,
    ):
        self.gui_vm = VsockClient(
            on_message=self.handle_request,
            on_connect=self.on_connect,
            on_disconnect=self.on_disconnect,
            cid=cid,
            port=port,
        )
        self.metadata = metadata
        self.passthrough_handler = passthrough_handler
        self.device_registry = {}
        self.lock = Lock()
        self.connected = False

    @log_entry_exit
    def start(self):
        self.gui_vm.start()

    @log_entry_exit
    def stop(self):
        self.gui_vm.stop()

    @log_entry_exit
    def wait(self):
        self.gui_vm.join()

    @log_entry_exit
    def on_connect(self):
        self.connected = True
        logger.info("Connected to GUI VM")

    @log_entry_exit
    def on_disconnect(self):
        if self.connected:
            logger.info("GUI VM Disconnected;")
        self.connected = False

    @log_entry_exit
    def notify_device_connected(
        self,
        device_id: str,
        vendor: str,
        product: str,
        permitted_vms: list[str],
        current_vm: str,
    ) -> bool:
        with self.lock:
            if device_id not in self.device_registry:
                self.device_registry[device_id] = {
                    "device_id": device_id,
                    "vendor": vendor,
                    "product": product,
                    "permitted_vms": permitted_vms,
                    "current_vm": current_vm,
                }
            else:
                logger.error(
                    f"Device {device_id} already in registry, connection request ignored."
                )
                return True

        device_connected_msg = gui_schema.DeviceConnected(
            device_id, vendor, product, permitted_vms, current_vm
        )
        device_connected_msg.print()
        if not self.gui_vm.send(device_connected_msg.to_json()):
            return False
        return True

    @log_entry_exit
    def reset(self) -> bool:
        with self.lock:
            self.device_registry.clear()
        reset_msg = gui_schema.Reset()
        if not self.gui_vm.send(reset_msg):
            return False
        return True

    @log_entry_exit
    def notify_device_disconnected(self, device_id: str):
        with self.lock:
            if device_id not in self.device_registry:
                logger.error(
                    f"Device {device_id} not found in registry, disconnection request ignored."
                )
                return False
            else:
                del self.device_registry[device_id]
        device_removed_msg = gui_schema.DeviceRemoved(device_id)
        if not self.gui_vm.send(device_removed_msg.to_json()):
            return False

        return True

    @log_entry_exit
    def notify_device_passthrough(self, device_id: str, new_vm: str):
        with self.lock:
            if device_id not in self.device_registry:
                logger.error(
                    f"Device {device_id} not found in registry, passthrough request ignored."
                )
                return False
            else:
                if new_vm in self.device_registry[device_id]["permitted_vms"]:
                    if new_vm != self.device_registry[device_id]["current_vm"]:
                        self.device_registry[device_id]["current_vm"] = new_vm
                    else:
                        logger.info(f"Device {device_id} already on VM {new_vm}")
                        return True
                else:
                    logger.error(f"Device {device_id} not permitted on VM {new_vm}")
                    return False
        passthrough_ack_msg = gui_schema.PassthroughAck(device_id, new_vm, "ok")

        if not self.gui_vm.send(passthrough_ack_msg.to_json()):
            return False
        return True

    @log_entry_exit
    def handle_request(self, msg: dict[str, Any]):
        logger.debug(f"Received request: {msg}")
        msgtype = gui_schema.get_message_type(msg)
        if msgtype == gui_schema.GetDevices.MSG_TYPE:
            connected_devices = gui_schema.ConnectedDevices(self.device_registry)
            if not self.gui_vm.send(connected_devices.to_json()):
                logger.error("System error! Service restart required.")
        elif msgtype == gui_schema.PassthroughRequest.MSG_TYPE:
            passthrough_request = gui_schema.PassthroughRequest.from_message(msg)
            device_id = passthrough_request.device_id
            target_vm = passthrough_request.target_vm
            if device_id in self.device_registry:
                if self.passthrough_handler(self.metadata, device_id, target_vm):
                    if not self.notify_device_passthrough(device_id, target_vm):
                        logger.error("Notify error! Service restart required.")
                    else:
                        logger.info(
                            f"Device {device_id} passed through to VM {target_vm}"
                        )
                else:
                    logger.error("Passthrough error! Service restart required")
            else:
                logger.error(f"Device {device_id} not found in registry")
        else:
            logger.error(f"Unknown msg: {msg}")
