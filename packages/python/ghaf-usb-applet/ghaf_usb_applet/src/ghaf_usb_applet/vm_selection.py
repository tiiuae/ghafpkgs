# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gtk, Gio, Gdk, GLib

from ghaf_usb_applet.logger import logger
from ghaf_usb_applet.api_client import APIClient

SELECT = "Select"


class DeviceSetting(Gtk.Application):
    def __init__(
        self, device: dict, apiclient: APIClient, title: str, app_id="ghaf.usb.setting"
    ):
        super().__init__(application_id=app_id, flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.device = device or {}
        self.apiclient = apiclient
        self.win = None
        self.title = title

    def do_activate(self):
        if self.win:
            self.win.present()
            return

        self.win = Gtk.ApplicationWindow(application=self, title=self.title)
        self.win.set_resizable(False)
        self.win.set_default_size(360, 160)

        key = Gtk.EventControllerKey()
        key.connect("key-pressed", self._on_key_pressed)
        self.win.add_controller(key)

        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        outer.set_margin_top(12)
        outer.set_margin_bottom(12)
        outer.set_margin_start(12)
        outer.set_margin_end(12)
        self.win.set_child(outer)

        product = self.device.get("product_name") or "USB Device"
        lbl_title = Gtk.Label(xalign=0)
        lbl_title.set_markup(f"<b>New device:</b> {product}")
        outer.append(lbl_title)

        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        outer.append(row)

        lbl_dd = Gtk.Label(label="Target:", xalign=0)
        lbl_dd.set_width_chars(8)
        row.append(lbl_dd)

        allowed = list(self.device.get("allowed_vms") or [])

        current = self.device.get("vm", "")
        options = allowed
        if current not in allowed:
            options = options + [SELECT]
            selected_idx = len(allowed)
        else:
            selected_idx = allowed.index(current)

        model = Gtk.StringList.new(options)
        dropdown = Gtk.DropDown.new(model=model, expression=None)
        dropdown.set_selected(selected_idx)
        dropdown.set_hexpand(True)
        row.append(dropdown)

        device_id = self.device.get("device_node", "")
        dropdown.connect("notify::selected", self._on_selected, device_id, allowed)

        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        actions.add_css_class("linked")
        actions.set_halign(Gtk.Align.END)
        outer.append(actions)

        btn_close = Gtk.Button(label="Close")
        btn_close.connect("clicked", lambda *_: self.win.close())
        actions.append(btn_close)
        self.win.present()

    def _on_selected(
        self, dropdown: Gtk.DropDown, _pspec, device_id: str, allowed: list
    ):
        idx = dropdown.get_selected()
        choice = dropdown.get_model().get_string(idx)
        if choice == SELECT:
            self.apiclient.usb_detach(device_id)
            return

        if choice not in allowed:
            logger.error(f"Invalid choice, Selected:{choice} Allowed:{allowed}")
            return

        if choice == self.device.get("vm"):
            logger.info(f"Device already passed to the VM:{choice}")
            return

        if device_id:
            logger.info(f"Device PASS req to the VM:{choice} for device: {device_id}")
            res = self.apiclient.usb_attach(device_id, choice)
            logger.info(f"Device PASS response:{res}")
            if res.get("event", "") == "usb_attached" or res.get("result", "") == "ok":
                dropdown.set_selected(idx)
                self.device["vm"] = choice
            else:
                GLib.idle_add(self._notify_error, "Device Error", f"Message: {res}")

    def _on_key_pressed(self, _ctrl, keyval, _keycode, _state):
        if keyval == Gdk.KEY_Escape:
            self.win.close()
            return True
        return False

    def _notify_error(self, title: str, msg: str) -> None:
        dlg = Gtk.AlertDialog()
        dlg.set_message(title)
        dlg.set_detail(msg)
        dlg.set_modal(True)
        dlg.show(self.win)


def show_device_setting(
    device: dict, title: str, apiclient: APIClient = None, port: int = 2000
):
    client = apiclient
    if apiclient is None:
        client = APIClient(port=port)
        client.connect()
    app = DeviceSetting(device=device, apiclient=client, title=title)
    raise SystemExit(app.run(None))
