import logging
import threading
import sys
from typing import List, Optional, Callable
from dataclasses import dataclass

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Gio

from upm.logger import logger

SELECT_LABEL = "Select"


@dataclass
class DeviceStruct:
    passthrough_handler: Callable[
        [str, str], bool
    ]  # callable(device_id, new_vm) -> bool
    device_id: str
    vendor: str
    product: str
    permitted_vms: List[str]
    current_vm: Optional[str] = None


def popup_thread_func(dev_struct: DeviceStruct):
    app = NotifyUser(dev_struct=dev_struct)
    sys.exit(app.run(None))


def show_new_device_popup_async(dev_struct: DeviceStruct):
    th = threading.Thread(target=popup_thread_func, args=(dev_struct,))
    th.start()
    th.join()


class PopupWindow(Gtk.ApplicationWindow):
    def __init__(self, app: Gtk.Application, dev_struct: DeviceStruct):
        super().__init__(application=app, title="Notification")
        self.set_default_size(320, 320)

        self.dev_struct = dev_struct
        self.blocks: dict[str, dict[str, Gtk.Widget]] = {}

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        self.set_child(root)

        content_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        content_box.set_margin_top(8)
        content_box.set_margin_bottom(8)
        content_box.set_margin_start(8)
        content_box.set_margin_end(8)
        content_box.set_hexpand(True)
        content_box.set_vexpand(True)
        root.append(content_box)

        notice_lbl = Gtk.Label(label="New device detected!")
        notice_lbl.set_margin_top(10)
        notice_lbl.set_xalign(0.5)
        notice_lbl.add_css_class("heading")
        content_box.append(notice_lbl)

        notice_lbl = Gtk.Label(label="Select the VM to passthrough to:")
        notice_lbl.set_margin_top(10)
        notice_lbl.set_xalign(0.5)
        content_box.append(notice_lbl)

        scroller = Gtk.ScrolledWindow()
        scroller.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroller.set_hexpand(True)
        scroller.set_vexpand(True)
        content_box.append(scroller)

        self.inner = Gtk.FlowBox()
        self.inner.set_selection_mode(Gtk.SelectionMode.NONE)
        self.inner.set_valign(Gtk.Align.CENTER)
        self.inner.set_halign(Gtk.Align.CENTER)
        self.inner.set_column_spacing(8)
        self.inner.set_row_spacing(8)
        self.inner.set_homogeneous(False)
        self.inner.set_min_children_per_line(1)
        self.inner.set_max_children_per_line(2)
        scroller.set_child(self.inner)

        action_bar = Gtk.ActionBar()
        root.append(action_bar)

        action_bar.pack_start(Gtk.Box(hexpand=True))
        self.close_btn = Gtk.Button(label="Close")
        self.close_btn.connect("clicked", lambda *_: self.close())
        action_bar.pack_end(self.close_btn)

        self.connect("close-request", self._on_close_request)
        self._add_block_ui(
            dev_struct.device_id,
            dev_struct.product,
            dev_struct.permitted_vms,
            dev_struct.current_vm,
        )

    def _make_dropdown(
        self, device_id: str, items: List[str], selected: Optional[str]
    ) -> Gtk.DropDown:
        strings = [SELECT_LABEL] + items
        dropdown = Gtk.DropDown.new_from_strings(strings)
        dropdown.set_hexpand(False)

        if selected and selected in items:
            dropdown.set_selected(
                items.index(selected) + 1
            )  # +1 because of SELECT_LABEL
        else:
            dropdown.set_selected(0)

        dropdown.connect("notify::selected", self._on_dropdown_changed, device_id)
        return dropdown

    def _add_block_ui(
        self,
        device_id: str,
        product: str,
        targets: List[str],
        selected: Optional[str],
    ) -> None:
        frame = Gtk.Frame()
        frame.add_css_class("card")
        frame.set_hexpand(True)
        frame.set_vexpand(False)

        vbox = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        vbox.set_margin_top(12)
        vbox.set_margin_bottom(12)
        vbox.set_margin_start(12)
        vbox.set_margin_end(12)
        frame.set_child(vbox)

        lbl = Gtk.Label()
        notice_lbl = Gtk.Label(label=f"{product}:")
        notice_lbl.set_xalign(0.5)
        vbox.append(notice_lbl)

        dropdown = self._make_dropdown(device_id, targets, selected)
        vbox.append(dropdown)

        self.inner.append(frame)

        self.blocks[device_id] = {
            "container": frame,
            "label": lbl,
            "dropdown": dropdown,
        }
        logger.debug("Added UI block for device %s with targets %s", device_id, targets)

    def _request_passthrough(self, device_id: str, new_vm: str) -> bool:
        try:
            ok = bool(self.dev_struct.passthrough_handler(device_id, new_vm))
        except Exception as e:
            logger.exception("Passthrough handler raised: %s", e)
            ok = False

        if not ok:
            self._show_error_dialog(
                title="Error", message="Failed to request passthrough."
            )
        return ok

    def _on_dropdown_changed(
        self, dropdown: Gtk.DropDown, _pspec, device_id: str
    ) -> None:
        idx = dropdown.get_selected()
        if idx < 0:
            return
        model = dropdown.get_model()
        try:
            text = model.get_string(idx)
        except AttributeError:
            text = dropdown.get_selected_item().get_string()

        if not text or text == SELECT_LABEL:
            return

        self._request_passthrough(device_id, text)

    def _show_error_dialog(self, title: str, message: str) -> bool:
        dlg = Gtk.MessageDialog(
            transient_for=self,
            modal=True,
            message_type=Gtk.MessageType.ERROR,
            buttons=Gtk.ButtonsType.CLOSE,
            text=title,
            secondary_text=message,
        )

        dlg.connect("response", lambda d, _r: d.destroy())
        dlg.present()
        return False

    def _on_close_request(self, *_args) -> bool:
        return False


class NotifyUser(Gtk.Application):
    def __init__(self, dev_struct: DeviceStruct):
        super().__init__(
            application_id="ghaf.notify.user", flags=Gio.ApplicationFlags.FLAGS_NONE
        )
        self._dev_struct = dev_struct
        self._win: Optional[PopupWindow] = None

    def do_activate(self):
        if not self._win:
            self._win = PopupWindow(self, dev_struct=self._dev_struct)
        self._win.present()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)

    def passthrough_handler(device_id: str, new_vm: str) -> bool:
        print(f"Device {device_id} passed to VM {new_vm}")
        return False  ##Test Error dialog

    device_struct = DeviceStruct(
        passthrough_handler=passthrough_handler,
        device_id="usb-1234:abcd",
        vendor="Vendor",
        product="Product",
        permitted_vms=["VM1", "VM2", "VM3"],
        current_vm="VM2",
    )

    app = NotifyUser(dev_struct=device_struct)
    raise SystemExit(app.run(None))
