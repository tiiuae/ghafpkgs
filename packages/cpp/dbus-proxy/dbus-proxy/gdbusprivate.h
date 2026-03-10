/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */
#ifndef GDBUS_PRIVATE_H
#define GDBUS_PRIVATE_H

constexpr const gchar *DBUS_INTERFACE_OBJECT_MANAGER =
    "org.freedesktop.DBus.ObjectManager";
constexpr const gchar *DBUS_SIGNAL_INTERFACES_ADDED = "InterfacesAdded";
constexpr const gchar *DBUS_SIGNAL_INTERFACES_REMOVED = "InterfacesRemoved";
constexpr const gchar *DBUS_OBJECT_PATH_DBUS = "/org/freedesktop/DBus";
constexpr const gchar *DBUS_INTERFACE_INTROSPECTABLE =
    "org.freedesktop.DBus.Introspectable";
constexpr const gchar *DBUS_INTERFACE_PROPERTIES =
    "org.freedesktop.DBus.Properties";
constexpr const gchar *DBUS_INTERFACE_PEER = "org.freedesktop.DBus.Peer";

constexpr const gchar *DBUS_NETWORK_MANAGER_NAME =
    "org.freedesktop.NetworkManager";
constexpr const gchar *DBUS_INTERFACE_SECRET_AGENT =
    "org.freedesktop.NetworkManager.SecretAgent";
constexpr const gchar *DBUS_NM_AGENT_PATH =
    "/org/freedesktop/NetworkManager/SecretAgent";

constexpr const gchar *DBUS_BT_AGENT_PATH = "/org/bluez/agent";
constexpr const gchar *DBUS_INTERFACE_BT_AGENT = "org.bluez.Agent1";

#endif // GDBUS_PRIVATE_H
