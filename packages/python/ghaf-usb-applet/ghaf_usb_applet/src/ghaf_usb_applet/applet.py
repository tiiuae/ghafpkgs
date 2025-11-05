# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("AyatanaAppIndicator3", "0.1")
from gi.repository import AyatanaAppIndicator3 as AppIndicator3
from gi.repository import Gtk, GLib

from ghaf_usb_applet.logger import logger
from ghaf_usb_applet.api_client import APIClient
from ghaf_usb_applet.notification_handler import USBDeviceNotification

import threading
import subprocess


class USBApplet:
    def __init__(self, port=2000):
        self.device_map = {}
        self.radio_groups = {}
        self.apiclient = APIClient(port=port)
        self.apiclient.connect()
        self.lock = threading.RLock()

        self.indicator = AppIndicator3.Indicator.new(
            "usb-applet",
            "drive-removable-media-usb",
            AppIndicator3.IndicatorCategory.APPLICATION_STATUS,
        )
        self.indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)

        self.menu = Gtk.Menu()
        self.indicator.set_menu(self.menu)
        self.refresh_device_list(async_=True, notify_error=True)
        self.menu.show_all()
        self.settings_icon = Gtk.Image.new_from_icon_name(
            "preferences-system-symbolic", Gtk.IconSize.MENU
        )

    def on_vm_toggled(self, menuitem, devname):
        with self.lock:
            if menuitem.get_active():
                device = self.device_map[devname]
                device_node = device.get("device_node", "")
                vm = menuitem.get_label()
                if vm.lower() == "none":
                    self.apiclient.usb_detach(device_node)
                else:
                    res = self.apiclient.usb_attach(device_node, vm)
                    if (
                        res.get("event", "") == "usb_attached"
                        or res.get("result", "") == "ok"
                    ):
                        logger.info(f"{devname} passed to {vm}")
                    else:
                        self.refresh_device_list()
                        GLib.idle_add(
                            self._notify_error, "Device Error", f"Message: {res}"
                        )

    def _build_devices_submenu(self):
        submenu = self.menu
        self.radio_groups.clear()
        for dev_name, dev in self.device_map.items():
            dev_top = Gtk.MenuItem(label=dev_name)
            devicemenu = Gtk.Menu()
            self.radio_groups[dev_name] = dev_top
            submenu.append(dev_top)
            allowed_vms = dev.get("allowed_vms", [])
            selected = dev.get("vm", None)
            radio_group = None
            for vm in allowed_vms:
                if radio_group is None:
                    radio_item = Gtk.RadioMenuItem.new_with_label(None, vm)
                    radio_group = radio_item
                else:
                    radio_item = Gtk.RadioMenuItem.new_with_label_from_widget(
                        radio_group, vm
                    )
                if vm == selected:
                    radio_item.set_active(True)
                radio_item.connect("toggled", self.on_vm_toggled, dev_name)
                devicemenu.append(radio_item)
            dev_top.set_submenu(devicemenu)

        settings_item = Gtk.ImageMenuItem(label="Settings")
        settings_item.connect("activate", self.open_settings)
        settings_item.set_image(self.settings_icon)
        settings_item.set_always_show_image(True)
        submenu.append(settings_item)
        submenu.show_all()

    def open_settings(self, *_):
        subprocess.Popen(["usb_settings"])
        self.menu.popdown()
        Gtk.MenuShell.deactivate(self.menu)

    def clear_menu(self):
        for child in self.menu.get_children():
            self.menu.remove(child)

    def refresh_device_list(self, notify_error=False, async_=True):
        def _fetch_and_update():
            import time

            for i in range(5):
                try:
                    devs = self.apiclient.get_devices_pretty()
                except Exception as e:
                    if i < 4:
                        logger.error(
                            f"Failed to fetch device list, trying again({i}). Error {e}"
                        )
                        time.sleep(2)
                        continue
                    else:
                        if notify_error:
                            self._notify_error(
                                "Server Error!", f"Device fetch failed: {e}"
                            )
                        return

            def _apply():
                self.clear_menu()
                self.device_map = devs or {}
                self.radio_groups.clear()
                self._build_devices_submenu()
                return GLib.SOURCE_REMOVE

            with self.lock:
                GLib.idle_add(_apply)

        if async_:
            threading.Thread(target=_fetch_and_update, daemon=True).start()
        else:
            _fetch_and_update()

    def _notify_error(self, title: str, msg: str) -> None:
        dialog = Gtk.MessageDialog(
            parent=None,
            flags=Gtk.DialogFlags.MODAL,
            type=Gtk.MessageType.ERROR,
            buttons=Gtk.ButtonsType.OK,
            message_format=title,
        )
        dialog.format_secondary_text(msg)
        dialog.run()
        dialog.destroy()


def start_usb_applet(port=2000):
    applet = USBApplet(port=port)
    notif = USBDeviceNotification(server_port=port)
    th = notif.monitor(applet.refresh_device_list)
    Gtk.main()
    th.join(timeout=2)
