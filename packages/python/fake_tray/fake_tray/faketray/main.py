# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

#!/usr/bin/env python3
import dbus
import dbus.service
import dbus.mainloop.glib
from gi.repository import GLib

SNI_IFACE = "org.kde.StatusNotifierItem"


class FakeTrayIcon(dbus.service.Object):
    def __init__(self, bus, path="/StatusNotifierItem"):
        super().__init__(bus, path)

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="ss", out_signature="v")
    def Get(self, interface, prop):
        props = {
            "Category": "ApplicationStatus",
            "Id": "example-tray-icon",
            "Title": "Example Tray Icon",
            "Status": "Active",
            "IconName": "dialog-information",
            "Menu": dbus.ObjectPath("/NO_MENU"),
        }
        return props.get(prop, "")

    @dbus.service.method(dbus.PROPERTIES_IFACE, in_signature="s", out_signature="a{sv}")
    def GetAll(self, interface):
        return {
            "Category": "ApplicationStatus",
            "Id": "example-tray-icon",
            "Title": "Example Tray Icon",
            "Status": "Active",
            "IconName": "dialog-information",
            "Menu": dbus.ObjectPath("/NO_MENU"),
            "IconPixmap": dbus.Array([], signature="(iiay)"),
            "OverlayIconName": "",
            "AttentionIconName": "",
            "ItemIsMenu": False,
        }

    @dbus.service.method(SNI_IFACE, in_signature="ii")
    def Activate(self, x, y):
        print(f"Clicked! x={x}, y={y}")

    @dbus.service.method(SNI_IFACE, in_signature="ii")
    def SecondaryActivate(self, x, y):
        print(f"Middle click! x={x}, y={y}")

    @dbus.service.method(SNI_IFACE, in_signature="is")
    def Scroll(self, delta, orientation):
        print(f"Scroll! delta={delta}, orientation={orientation}")


def main():
    dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()

    bus_name = dbus.service.BusName("org.kde.StatusNotifierItem-99999-1", bus)  # noqa: F841
    icon = FakeTrayIcon(bus)  # noqa: F841
    print("Fake tray icon created: org.kde.StatusNotifierItem-99999-1")

    try:
        watcher = bus.get_object(
            "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher"
        )
        watcher.RegisterStatusNotifierItem(
            "org.kde.StatusNotifierItem-99999-1",
            dbus_interface="org.kde.StatusNotifierWatcher",
        )
        print("Registered with watcher!")
    except Exception as e:
        print(f"Watcher registration failed (normal if no SNI watcher): {e}")

    print("Running... Ctrl+C to exit")
    GLib.MainLoop().run()
