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

constexpr const gchar *DBUS_OBJECT_PATH_NETWORK_MANAGER =
    "/org/freedesktop/NetworkManager";

// SNI (StatusNotifierItem) protocol constants
constexpr const gchar *SNI_WATCHER_INTERFACE = "org.kde.StatusNotifierWatcher";
constexpr const gchar *SNI_WATCHER_BUS_NAME = "org.kde.StatusNotifierWatcher";
constexpr const gchar *SNI_WATCHER_OBJECT_PATH = "/StatusNotifierWatcher";
constexpr const gchar *SNI_ITEM_INTERFACE = "org.kde.StatusNotifierItem";
constexpr const gchar *SNI_ITEM_OBJECT_PATH = "/StatusNotifierItem";
constexpr const gchar *SNI_ITEM_BUS_NAME_PREFIX = "org.kde.StatusNotifierItem-";
constexpr const gchar *SNI_SIGNAL_ITEM_REGISTERED =
    "StatusNotifierItemRegistered";
constexpr const gchar *SNI_SIGNAL_ITEM_UNREGISTERED =
    "StatusNotifierItemUnregistered";
constexpr const gchar *DBUS_INTERFACE_DBUS = "org.freedesktop.DBus";
constexpr const gchar *DBUS_SIGNAL_NAME_OWNER_CHANGED = "NameOwnerChanged";
constexpr const gchar *DBUSMENU_INTERFACE = "com.canonical.dbusmenu";

#endif // GDBUS_PRIVATE_H
