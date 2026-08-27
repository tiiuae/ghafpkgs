# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, Gio, GLib, Gtk

from ghaf_usb_applet.api_client import DEFAULT_PORT, APIClient
from ghaf_usb_applet.logger import logger


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
        self.win.set_default_size(360, 200)

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

        lbl_target = Gtk.Label(label="Attached to:", xalign=0)
        outer.append(lbl_target)

        allowed = list(self.device.get("allowed_vms") or [])
        if "None" not in allowed and "none" not in allowed:
            allowed.append("None")

        current = self.device.get("vm") or "None"
        device_id = self.device.get("device_node", "")

        radio_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        outer.append(radio_box)

        group_head = None
        for vm in allowed:
            btn = Gtk.CheckButton.new_with_label(vm)
            if group_head is None:
                group_head = btn
            else:
                btn.set_group(group_head)
            btn.set_active(vm == current)
            btn.connect("toggled", self._on_toggled, device_id, vm)
            radio_box.append(btn)

        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        actions.add_css_class("linked")
        actions.set_halign(Gtk.Align.END)
        outer.append(actions)

        btn_close = Gtk.Button(label="Close")
        btn_close.connect("clicked", lambda *_: self.win.close())
        actions.append(btn_close)
        self.win.present()

    def _on_toggled(self, btn: Gtk.CheckButton, device_id: str, choice: str):
        if not btn.get_active() or choice == self.device.get("vm"):
            return

        if choice.lower() == "none":
            self.apiclient.usb_detach(device_id)
            self.device["vm"] = choice
            return

        if device_id:
            logger.info(f"Requesting passthrough of {device_id} to VM '{choice}'")
            res = self.apiclient.usb_attach(device_id, choice)
            logger.debug("Passthrough response: %s", res)
            if res.get("event", "") == "usb_attached" or res.get("result", "") == "ok":
                self.device["vm"] = choice
            else:
                GLib.idle_add(
                    self._notify_error,
                    "Device Error",
                    res.get("error", "Unknown error"),
                )

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
    device: dict, title: str, apiclient: APIClient = None, port: int = DEFAULT_PORT
):
    client = apiclient
    if apiclient is None:
        client = APIClient(port=port)
        client.connect()
    app = DeviceSetting(device=device, apiclient=client, title=title)
    raise SystemExit(app.run(None))
