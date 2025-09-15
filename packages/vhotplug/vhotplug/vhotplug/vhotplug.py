# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import logging
import asyncio
import argparse
import os
import pyudev
import copy
import threading
from vhotplug.device import (
    vm_for_usb_device,
    attach_usb_device,
    remove_usb_device,
    log_device,
    is_usb_device,
    get_usb_info,
    attach_connected_devices,
)
from vhotplug.config import Config
from vhotplug.filewatcher import FileWatcher
from vhotplug.apiserver import APIServer

logger = logging.getLogger("vhotplug")


class UserDevices:
    def __init__(self) -> None:
        self._devices = {}
        self._lock = threading.RLock()

    def add(self, id, usbInfo):
        with self._lock:
            self._devices[id] = copy.deepcopy(usbInfo)

    def get(self, id):
        with self._lock:
            return self._devices.get(id)

    def delete(self, id):
        with self._lock:
            if id in self._devices:
                del self._devices[id]
                return True
            return False

    def list_ids(self):
        with self._lock:
            return list(self._devices.keys())

    def has(self, id):
        with self._lock:
            return id is not None and id in self._devices


userDevices = UserDevices()


async def device_event(context, config, device, api_server):
    if device.action == "add":
        logger.debug("Device plugged: %s", device.sys_name)
        logger.debug("Subsystem: %s, path: %s", device.subsystem, device.device_path)
        log_device(device)
        if is_usb_device(device):
            usb_info = get_usb_info(device)
            logger.info(
                "USB device %s:%s (%s %s) connected: %s",
                usb_info.vid,
                usb_info.pid,
                usb_info.vendor_name,
                usb_info.product_name,
                device.device_node,
            )
            logger.info(
                'Device class: "%s", subclass: "%s", protocol: "%s", interfaces: "%s"',
                usb_info.device_class,
                usb_info.device_subclass,
                usb_info.device_protocol,
                usb_info.interfaces,
            )
            try:
                vm = await vm_for_usb_device(
                    context, config, api_server, usb_info, None, True
                )
                if vm:
                    await attach_usb_device(config, api_server, usb_info, vm)
            except RuntimeError as e:
                logger.error("Failed to attach device: %s", e)
    elif device.action == "remove":
        logger.debug("Device unplugged: %s", device.sys_name)
        logger.debug("Subsystem: %s, path: %s", device.subsystem, device.device_path)
        log_device(device)
        if is_usb_device(device):
            usb_info = get_usb_info(device)
            logger.info("USB device disconnected: %s", device.device_node)
            try:
                await remove_usb_device(config, usb_info, api_server)
            except RuntimeError as e:
                logger.error("Failed to detach device: %s", e)
    elif device.action == "change":
        logger.debug("Device changed: %s", device.sys_name)
        logger.debug("Subsystem: %s, path: %s", device.subsystem, device.device_path)
        if device.subsystem == "power_supply":
            logger.info(
                "Power supply device %s changed, this may indicate a system resume",
                device.sys_name,
            )


async def device_event_with_upm(context, config, device, upm_host):
    global userDevices
    if device.action == "add":
        logger.debug("Device plugged: %s", device.sys_name)
        logger.debug("Subsystem: %s, path: %s", device.subsystem, device.device_path)
        log_device(device)
        if is_usb_device(device):
            usb_info = get_usb_info(device)
            logger.info(
                "USB device %s:%s (%s %s) connected: %s",
                usb_info.vid,
                usb_info.pid,
                usb_info.vendor_name,
                usb_info.product_name,
                device.device_node,
            )
            logger.info(
                'Device class: "%s", subclass: "%s", protocol: "%s", interfaces: "%s"',
                usb_info.device_class,
                usb_info.device_subclass,
                usb_info.device_protocol,
                usb_info.interfaces,
            )
            try:
                res = config.vm_for_usb_device(usb_info)
                if not res:
                    logger.info("No VM found for %s:%s", usb_info.vid, usb_info.pid)
                    return None
                permitted = res[1]
                if len(permitted) == 0:
                    vm = await vm_for_usb_device(
                        context, config, None, usb_info, None, True
                    )
                    await attach_usb_device(config, None, usb_info, vm)
                else:
                    device_id = usb_info.dev_id()
                    if not upm_host.notify_device_connected(
                        device_id,
                        usb_info.vendor_name,
                        usb_info.product_name,
                        permitted,
                        "",
                    ):
                        logger.error("Notify device connected failed")
                    else:
                        userDevices.add(device_id, usb_info)
                        logger.info(f"Device {device_id} connected")
            except RuntimeError as e:
                logger.error("Failed to attach device: %s", e)
    elif device.action == "remove":
        logger.debug("Device unplugged: %s", device.sys_name)
        logger.debug("Subsystem: %s, path: %s", device.subsystem, device.device_path)
        log_device(device)
        if is_usb_device(device):
            usb_info = get_usb_info(device)
            logger.info("USB device disconnected: %s", device.device_node)
            try:
                await remove_usb_device(config, usb_info, None)
                device_id = usb_info.dev_id()
                if userDevices.has(device_id):
                    if not upm_host.notify_device_disconnected(device_id):
                        logger.error("Notify device disconnected failed")
                    userDevices.delete(device_id)
            except RuntimeError as e:
                logger.error("Failed to detach device: %s", e)
    elif device.action == "change":
        logger.debug("Device changed: %s", device.sys_name)
        logger.debug("Subsystem: %s, path: %s", device.subsystem, device.device_path)
        if device.subsystem == "power_supply":
            logger.info(
                "Power supply device %s changed, this may indicate a system resume",
                device.sys_name,
            )


# pylint: disable = too-many-positional-arguments
async def monitor_loop(
    monitor, context, config, user_interface, watcher, attach_connected, with_upm
):
    while True:
        device = await asyncio.to_thread(monitor.poll, 1)
        if device:
            if with_upm:
                await device_event_with_upm(context, config, device, user_interface)
            else:
                await device_event(context, config, device, user_interface)

        if watcher.detect_restart() and attach_connected:
            await attach_connected_devices(context, config)


def handle_user_device_passthrough_request(metadata, device_id, new_vm):
    config, devices = metadata
    try:
        usb_info = devices.get(device_id)
        if not usb_info:
            logger.error("Device not found")
            return False
        vm = config.get_vm(new_vm)
        asyncio.run(remove_usb_device(config, usb_info, None))
        asyncio.run(attach_usb_device(config, None, usb_info, vm))
        return True
    except RuntimeError as e:
        logger.error("Failed to attach device: %s", e)
        return False


async def async_main():
    parser = argparse.ArgumentParser(
        description="Hot-plugging USB devices to the virtual machines"
    )
    parser.add_argument(
        "-c", "--config", type=str, required=True, help="Path to the configuration file"
    )
    parser.add_argument(
        "-a",
        "--attach-connected",
        default=False,
        action=argparse.BooleanOptionalAction,
        help="Attach connected devices on startup",
    )
    parser.add_argument(
        "-u",
        "--usb-passthrough-manager",
        default=True,
        action=argparse.BooleanOptionalAction,
        help="With USB passthrough manager",
    )
    parser.add_argument(
        "-t",
        "--target-cid",
        type=int,
        default=5,
        help="cid of target vm (gui-vm) to send hotplug events to",
    )
    parser.add_argument(
        "-p",
        "--target-port",
        type=int,
        default=7000,
        help="port of target vm (gui-vm) to send hotplug events to",
    )
    parser.add_argument(
        "-d",
        "--debug",
        default=False,
        action=argparse.BooleanOptionalAction,
        help="Enable debug messages",
    )
    args = parser.parse_args()

    handler = logging.StreamHandler()
    handler.setFormatter(logging.Formatter("%(levelname)s %(message)s"))
    logger.addHandler(handler)
    logger.setLevel(logging.DEBUG if args.debug else logging.INFO)

    if not os.path.exists(args.config):
        logger.error("Configuration file %s not found", args.config)
        return

    config = Config(args.config)

    context = pyudev.Context()
    if args.attach_connected:
        await attach_connected_devices(context, config)

    monitor = pyudev.Monitor.from_netlink(context)

    watcher = FileWatcher()
    for vm in config.get_all_vms():
        vm_socket = vm.get("socket")
        if vm_socket:
            watcher.add_file(vm_socket)

    server = None
    if args.usb_passthrough_manager:
        from upm.host.service import HostService
        from upm.logger import setup_logger as upm_logger_setup

        upm_logger_setup("debug" if args.debug else "info")
        server = HostService(
            port=args.target_port,
            cid=args.target_cid,
            passthrough_handler=handle_user_device_passthrough_request,
            metadata=(config, userDevices),
        )
        server.start()
    else:
        if config.api_enabled():
            server = APIServer(config, context, asyncio.get_event_loop())
            server.start()

    logger.info("Waiting for new devices")
    await monitor_loop(
        monitor,
        context,
        config,
        server,
        watcher,
        args.attach_connected,
        args.usb_passthrough_manager,
    )


def main():
    try:
        loop = asyncio.get_event_loop()
        loop.run_until_complete(async_main())
    except asyncio.CancelledError:
        logger.info("Cancelled by event loop")
    except KeyboardInterrupt:
        logger.info("Ctrl+C pressed")
    logger.info("Exiting")
