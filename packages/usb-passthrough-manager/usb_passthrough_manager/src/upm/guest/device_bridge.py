import json
import logging
from pathlib import Path
from typing import Any, Dict, List, Optional

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("GLib", "2.0")

from gi.repository import Gtk, Gio

logger = logging.getLogger("upm")

SELECT_LABEL = "Select"


def _read_schema_once(path: Path) -> Dict[str, Any]:
    try:
        with open(path, "r", encoding="utf-8") as f:
            doc = json.load(f) or {}
    except Exception as e:
        logger.error(f"Failed to read schema file: {e}")
        return {}
    return doc if isinstance(doc, dict) else {}


class AppWindow(Gtk.ApplicationWindow):
    def __init__(
        self,
        app: Gtk.Application,
        data_dir: str,
    ):
        super().__init__(application=app, title="Device Bridge")
        self.set_default_size(600, 300)

        self.file_path = Path(data_dir) / "usb_db.json"
        self.fifo_path = Path(data_dir) / "app_request.fifo"

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

        scroller = Gtk.ScrolledWindow()
        scroller.set_policy(Gtk.PolicyType.AUTOMATIC, Gtk.PolicyType.AUTOMATIC)
        scroller.set_hexpand(True)
        scroller.set_vexpand(True)
        content_box.append(scroller)

        self.inner = Gtk.FlowBox()
        self.inner.set_selection_mode(Gtk.SelectionMode.NONE)
        self.inner.set_valign(Gtk.Align.START)
        self.inner.set_halign(Gtk.Align.FILL)
        self.inner.set_column_spacing(8)
        self.inner.set_row_spacing(8)
        self.inner.set_homogeneous(False)
        self.inner.set_min_children_per_line(2)
        self.inner.set_max_children_per_line(2)
        scroller.set_child(self.inner)

        action_bar = Gtk.ActionBar()
        root.append(action_bar)

        action_bar.pack_start(Gtk.Box(hexpand=True))

        self.refresh_btn = Gtk.Button(label="Refresh")
        self.refresh_btn.set_tooltip_text("Refresh status from JSON.")
        self.refresh_btn.connect("clicked", self._on_refresh_clicked)
        action_bar.pack_end(self.refresh_btn)

        self.close_btn = Gtk.Button(label="Close")
        self.close_btn.connect("clicked", lambda *_: self.close())
        action_bar.pack_end(self.close_btn)

        self.connect("close-request", self._on_close_request)

        self.blocks = {}
        self._apply_reload_ui()

    def _clear_blocks_ui(self) -> None:
        for info in list(self.blocks.values()):
            container = info.get("container")
            if container is not None and container.get_parent() is not None:
                self.inner.remove(container)
        self.blocks.clear()

    def _make_dropdown(
        self, device_id: str, items: List[str], selected: Optional[str]
    ) -> Gtk.DropDown:
        model = Gtk.StringList.new([SELECT_LABEL] + items)
        dropdown = Gtk.DropDown.new(model=model, expression=None)
        dropdown.set_hexpand(False)
        if selected and selected in items:
            dropdown.set_selected(items.index(selected) + 1)
        else:
            dropdown.set_selected(0)
        dropdown.connect("notify::selected", self._on_dropdown_changed, device_id)
        return dropdown

    def _add_block_ui(
        self,
        device_id: str,
        vendor: str,
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
        lbl.set_use_markup(True)
        lbl.set_selectable(True)
        lbl.set_xalign(0.0)
        lbl.set_hexpand(True)
        lbl.set_markup(f"{product}:{vendor}")
        vbox.append(lbl)

        dropdown = self._make_dropdown(device_id, targets, selected)
        vbox.append(dropdown)

        self.inner.append(frame)

        self.blocks[device_id] = {
            "container": frame,
            "label": lbl,
            "dropdown": dropdown,
        }

    def _apply_reload_ui(self):
        doc = _read_schema_once(self.file_path)
        self._clear_blocks_ui()
        for dev_id, meta in doc.items():
            permitted = list(meta.get("permitted-vms", []))
            vendor = meta.get("vendor") or ""
            product = meta.get("product") or ""
            selected = meta.get("current-vm") or ""
            self._add_block_ui(dev_id, vendor, product, permitted, selected)

    def _request_passthrough(self, device_id: str, new_vm: str) -> bool:
        request = f"{device_id}->{new_vm}\n"
        with open(self.fifo_path, "w", encoding="utf-8", buffering=1) as f:
            try:
                f.write(request)
                return True
            except Exception as e:
                logger.error(f"Failed to send passthrough request: {e}")
                return False
        return False

    def _on_dropdown_changed(
        self, dropdown: Gtk.DropDown, _pspec, device_id: str
    ) -> None:
        idx = dropdown.get_selected()
        if idx < 0:
            return
        model = dropdown.get_model()
        text = model.get_string(idx)
        if text is None or text == SELECT_LABEL:
            return
        self._request_passthrough(device_id, text)

    def _on_refresh_clicked(self, _btn: Gtk.Button) -> None:
        self._apply_reload_ui()

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


class DeviceBridge(Gtk.Application):
    def __init__(self, data_dir: str):
        super().__init__(
            application_id="ghaf.device.bridge", flags=Gio.ApplicationFlags.FLAGS_NONE
        )
        self._data_dir = data_dir
        self._win: Optional[AppWindow] = None

    def do_activate(self):
        if not self._win:
            self._win = AppWindow(self, data_dir=self._data_dir)
        self._win.present()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    import sys

    data_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    app = DeviceBridge(data_dir=data_dir)
    raise SystemExit(app.run(None))
