#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
"""
Fake StatusNotifierItem for testing SNI proxy locally.

Usage:
  DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/test-sni-source.sock python3 test_sni_item.py

This script:
1. Owns the bus name "org.kde.StatusNotifierItem-99999-1"
2. Exports /StatusNotifierItem with the org.kde.StatusNotifierItem interface
3. Exports /com/canonical/dbusmenu with com.canonical.dbusmenu interface
4. Sits idle so the proxy can discover and introspect it
"""

import sys
import signal
from gi.repository import Gio, GLib

BUS_NAME = "org.kde.StatusNotifierItem-99999-1"

SNI_XML = """
<node>
  <interface name='org.kde.StatusNotifierItem'>
    <property name='Category' type='s' access='read'/>
    <property name='Id' type='s' access='read'/>
    <property name='Title' type='s' access='read'/>
    <property name='Status' type='s' access='read'/>
    <property name='IconName' type='s' access='read'/>
    <property name='Menu' type='o' access='read'/>
    <method name='Activate'>
      <arg name='x' type='i' direction='in'/>
      <arg name='y' type='i' direction='in'/>
    </method>
    <method name='SecondaryActivate'>
      <arg name='x' type='i' direction='in'/>
      <arg name='y' type='i' direction='in'/>
    </method>
    <method name='ContextMenu'>
      <arg name='x' type='i' direction='in'/>
      <arg name='y' type='i' direction='in'/>
    </method>
    <method name='Scroll'>
      <arg name='delta' type='i' direction='in'/>
      <arg name='orientation' type='s' direction='in'/>
    </method>
    <signal name='NewTitle'/>
    <signal name='NewIcon'/>
    <signal name='NewStatus'>
      <arg name='status' type='s'/>
    </signal>
  </interface>
</node>
"""

DBUSMENU_XML = """
<node>
  <interface name='com.canonical.dbusmenu'>
    <method name='GetLayout'>
      <arg name='parentId' type='i' direction='in'/>
      <arg name='recursionDepth' type='i' direction='in'/>
      <arg name='propertyNames' type='as' direction='in'/>
      <arg name='revision' type='u' direction='out'/>
      <arg name='layout' type='(ia{sv}av)' direction='out'/>
    </method>
    <signal name='LayoutUpdated'>
      <arg name='revision' type='u'/>
      <arg name='parent' type='i'/>
    </signal>
    <property name='Version' type='u' access='read'/>
    <property name='Status' type='s' access='read'/>
  </interface>
</node>
"""

sni_node_info = None
menu_node_info = None


def on_sni_method_call(
    connection, sender, object_path, interface_name, method_name, parameters, invocation
):
    print(f"[SNI] Method call: {interface_name}.{method_name} from {sender}")
    if method_name in ("Activate", "SecondaryActivate", "ContextMenu", "Scroll"):
        invocation.return_value(None)
    else:
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.UnknownMethod", f"Unknown method: {method_name}"
        )


def on_sni_get_property(connection, sender, object_path, interface_name, property_name):
    print(f"[SNI] Get property: {property_name} from {sender}")
    props = {
        "Category": GLib.Variant("s", "ApplicationStatus"),
        "Id": GLib.Variant("s", "test-sni-app"),
        "Title": GLib.Variant("s", "Test SNI App"),
        "Status": GLib.Variant("s", "Active"),
        "IconName": GLib.Variant("s", "dialog-information"),
        "Menu": GLib.Variant("o", "/com/canonical/dbusmenu"),
    }
    return props.get(property_name)


def on_menu_method_call(
    connection, sender, object_path, interface_name, method_name, parameters, invocation
):
    print(f"[Menu] Method call: {interface_name}.{method_name} from {sender}")
    if method_name == "GetLayout":
        # Return a minimal empty menu layout
        layout = GLib.Variant("(u(ia{sv}av))", (1, (0, {}, [])))
        invocation.return_value(layout)
    else:
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.UnknownMethod", f"Unknown method: {method_name}"
        )


def on_menu_get_property(
    connection, sender, object_path, interface_name, property_name
):
    print(f"[Menu] Get property: {property_name} from {sender}")
    props = {
        "Version": GLib.Variant("u", 3),
        "Status": GLib.Variant("s", "normal"),
    }
    return props.get(property_name)


def on_bus_acquired(connection, name):
    global sni_node_info, menu_node_info

    print("[*] Bus acquired, registering objects...")

    # Register SNI object
    sni_node_info = Gio.DBusNodeInfo.new_for_xml(SNI_XML)
    connection.register_object(
        "/StatusNotifierItem",
        sni_node_info.interfaces[0],
        on_sni_method_call,
        on_sni_get_property,
        None,
    )
    print("[*] Registered /StatusNotifierItem")

    # Register dbusmenu object
    menu_node_info = Gio.DBusNodeInfo.new_for_xml(DBUSMENU_XML)
    connection.register_object(
        "/com/canonical/dbusmenu",
        menu_node_info.interfaces[0],
        on_menu_method_call,
        on_menu_get_property,
        None,
    )
    print("[*] Registered /com/canonical/dbusmenu")


def on_name_acquired(connection, name):
    print(f"[*] Acquired bus name: {name}")

    # Register with watcher using UNIQUE name (simulates Electron/Chromium behavior)
    # This tests the proxy's resolve_unique_to_sni_name() path
    unique = connection.get_unique_name()
    try:
        connection.call_sync(
            "org.kde.StatusNotifierWatcher",
            "/StatusNotifierWatcher",
            "org.kde.StatusNotifierWatcher",
            "RegisterStatusNotifierItem",
            GLib.Variant("(s)", (unique,)),
            None,
            Gio.DBusCallFlags.NONE,
            5000,
            None,
        )
        print(f"[*] Registered with watcher using unique name: {unique}")
    except Exception as e:
        print(f"[*] Watcher registration skipped (normal if no watcher): {e}")

    print("[*] Fake SNI item is running. Press Ctrl+C to quit.")


def on_name_lost(connection, name):
    print(f"[!] Lost bus name: {name}")
    sys.exit(1)


def main():
    loop = GLib.MainLoop()

    Gio.bus_own_name(
        Gio.BusType.SESSION,
        BUS_NAME,
        Gio.BusNameOwnerFlags.NONE,
        on_bus_acquired,
        on_name_acquired,
        on_name_lost,
    )

    signal.signal(signal.SIGINT, lambda *_: loop.quit())
    signal.signal(signal.SIGTERM, lambda *_: loop.quit())

    print(f"[*] Starting fake SNI item: {BUS_NAME}")
    loop.run()
    print("[*] Exiting.")


if __name__ == "__main__":
    main()
