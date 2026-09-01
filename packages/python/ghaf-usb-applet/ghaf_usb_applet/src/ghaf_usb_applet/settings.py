# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")

from gi.repository import Gdk, GLib, Gtk, Pango

from ghaf_usb_applet.api_client import DEFAULT_PORT, APIClient
from ghaf_usb_applet.logger import logger


class OptionsPopover(Gtk.Popover):
    def __init__(self, parent_widget, options, selected, on_chosen):
        super().__init__(has_arrow=True)
        self.set_parent(parent_widget)
        self.set_position(Gtk.PositionType.RIGHT)
        self._selected = selected
        self._on_chosen = on_chosen

        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        box.set_margin_top(10)
        box.set_margin_bottom(10)
        box.set_margin_start(12)
        box.set_margin_end(12)
        self.set_child(box)

        group_head = None
        for opt in options:
            btn = Gtk.CheckButton.new_with_label(str(opt))
            if group_head is None:
                group_head = btn
            else:
                btn.set_group(group_head)
            btn.set_active(opt == selected)
            btn.connect("toggled", self._on_toggled, opt)
            box.append(btn)

        self.set_autohide(True)

    def _on_toggled(self, btn, opt):
        if not btn.get_active():
            return
        if opt != self._selected:
            self._selected = opt
            self._on_chosen(opt)
        self.popdown()


class DeviceSettings(Gtk.ApplicationWindow):
    def __init__(self, port=DEFAULT_PORT, **kwargs):
        super().__init__(**kwargs)
        self.apiclient = APIClient(port=port)
        self.apiclient.connect()
        self.set_title("USB Passthrough Settings")
        self.set_default_size(700, 400)
        self.set_resizable(False)
        self._active_popover = None

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        root.set_margin_top(20)
        root.set_margin_bottom(20)
        root.set_margin_start(22)
        root.set_margin_end(22)
        self.set_child(root)

        title_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        root.append(title_row)

        title = Gtk.Label(label="Attach USB devices to VMs")
        title.add_css_class("title-3")
        title.set_xalign(0.0)
        title.set_wrap(True)
        title.set_hexpand(True)
        title_row.append(title)

        refresh_button = Gtk.Button.new_from_icon_name("view-refresh-symbolic")
        refresh_button.set_tooltip_text("Refresh devices")
        refresh_button.connect("clicked", self.on_refresh_clicked)
        title_row.append(refresh_button)

        self.list = Gtk.ListBox()
        self.list.add_css_class("boxed-list")
        self.list.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.list.set_activate_on_single_click(True)
        self.list.connect("row-activated", self._on_row_activated)

        scroller = Gtk.ScrolledWindow()
        scroller.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroller.set_max_content_height(300)
        scroller.set_propagate_natural_height(True)
        scroller.set_child(self.list)
        root.append(scroller)

        self._model = {}
        self.refresh()

        kc = Gtk.EventControllerKey()
        kc.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        kc.connect("key-pressed", self._on_window_key)
        self.add_controller(kc)

    def on_refresh_clicked(self, widget):
        self.refresh()

    def _notify_error(self, title: str, msg: str) -> None:
        dlg = Gtk.AlertDialog()
        dlg.set_message(title)
        dlg.set_detail(msg)
        dlg.set_modal(True)
        dlg.show(self)

    def refresh(self):
        try:
            self._model = self.apiclient.get_devices_pretty()
            logger.debug("USB device inventory: %s", self._model)
        except Exception as e:  # noqa: BLE001 - GUI boundary: any failure is shown to the user
            logger.exception("Failed fetching devices")
            GLib.idle_add(self._notify_error, "Device Error", f"Message: {e}")
            return
        self._rebuild_rows()

    def _rebuild_rows(self):
        for ch in list(self.list):
            self.list.remove(ch)
        if not self._model:
            self.list.append(self._build_empty_row())
        else:
            for key, data in self._model.items():
                self.list.append(self._build_row(key, data))
        self.list.show()

    def _build_empty_row(self):
        row = Gtk.ListBoxRow()
        row.set_activatable(False)
        row.set_selectable(False)
        label = Gtk.Label(label="No USB devices detected")
        label.add_css_class("dim-label")
        label.set_margin_top(14)
        label.set_margin_bottom(14)
        row.set_child(label)
        return row

    def _build_row(self, l1_key, data):
        row = Gtk.ListBoxRow()
        row._l1_key = l1_key
        row.set_activatable(True)

        h = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        h.set_margin_top(14)
        h.set_margin_bottom(14)
        h.set_margin_start(16)
        h.set_margin_end(16)

        title = Gtk.Label(label=l1_key)
        title.set_xalign(0.0)
        title.set_hexpand(True)
        title.set_ellipsize(Pango.EllipsizeMode.END)
        title.set_max_width_chars(40)
        h.append(title)

        right = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        right.set_halign(Gtk.Align.END)
        lbl_target = Gtk.Label(label="Attached to:")
        lbl_target.add_css_class("dim-label")
        right.append(lbl_target)
        value = Gtk.Label(label=str(data.get("vm")))
        value.add_css_class("dim-label")
        right.append(value)
        chevron = Gtk.Image.new_from_icon_name("pan-down-symbolic")
        right.append(chevron)
        row._value_label = value

        h.append(right)
        row.set_child(h)
        return row

    def _open_popover_for_row(self, row):
        self.list.select_row(row)
        if self._active_popover:
            try:
                self._active_popover.popdown()
            except Exception as e:  # noqa: BLE001 - GTK raises freely when the popover is already gone
                # Non-fatal: the popover is already closed or invalid
                logger.debug(f"Ignoring popdown failure: {e}")
            self._active_popover = None

        key = getattr(row, "_l1_key", None)
        if not key:
            return
        entry = self._model.get(key, {})
        options = entry.get("allowed_vms", [])
        selected = entry.get("vm")

        pop = OptionsPopover(
            parent_widget=row,
            options=options,
            selected=selected,
            on_chosen=lambda opt, k=key, r=row: self._apply_choice(k, opt, r),
        )
        pop.connect("closed", self._on_popover_closed)

        self._active_popover = pop
        pop.popup()

    def _on_popover_closed(self, *_):
        self._active_popover = None
        self.list.grab_focus()

    def _on_row_activated(self, _lb, row):
        if row:
            self._open_popover_for_row(row)

    def _attach_to(self, device_name: str, new_vm: str):
        device = self._model.get(device_name, {})
        device_node = device.get("device_node", "")
        if new_vm.lower() == "none":
            self.apiclient.usb_detach(device_node)
        else:
            res = self.apiclient.usb_attach(device_node, new_vm)
            if res.get("event") == "usb_attached" or res.get("result") == "ok":
                device["vm"] = new_vm
                return True
            GLib.idle_add(
                self._notify_error,
                "Device Error",
                res.get("error", "Unknown error"),
            )
            return False
        return True

    def _apply_choice(self, l1_key, opt, row):
        cur = self._model.get(l1_key, {}).get("vm")
        if opt == cur:
            return
        if self._attach_to(l1_key, opt):
            self._model[l1_key]["vm"] = opt
            if hasattr(row, "_value_label"):
                row._value_label.set_text(str(opt))

    def _on_window_key(self, _ctl, keyval, *_):
        if keyval == Gdk.KEY_Escape:
            if self._active_popover is not None:
                try:
                    self._active_popover.popdown()
                finally:
                    self._active_popover = None
                    self.list.grab_focus()
                return True
            self.close()
            return True
        return False


class SettingsMenu(Gtk.Application):
    def __init__(self, port=DEFAULT_PORT):
        super().__init__(application_id="ghaf.usb.settings")
        self.port = port

    def do_activate(self, *_):
        win = DeviceSettings(application=self, port=self.port)
        win.present()
