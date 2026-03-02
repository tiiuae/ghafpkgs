/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#pragma once

constexpr const char *DBUS_INTERFACE_OBJECT_MANAGER =
    "org.freedesktop.DBus.ObjectManager";
constexpr const char *DBUS_SIGNAL_INTERFACES_ADDED = "InterfacesAdded";
constexpr const char *DBUS_SIGNAL_INTERFACES_REMOVED = "InterfacesRemoved";
constexpr const char *DBUS_OBJECT_PATH = "/org/freedesktop";
constexpr const char *DBUS_OBJECT_PATH_DBUS = "/org/freedesktop/DBus";
constexpr const char *DBUS_INTERFACE_INTROSPECTABLE =
    "org.freedesktop.DBus.Introspectable";
constexpr const char *DBUS_INTERFACE_PROPERTIES =
    "org.freedesktop.DBus.Properties";
constexpr const char *DBUS_INTERFACE_PEER = "org.freedesktop.DBus.Peer";
constexpr const char *DBUS_INTERFACE_SECRET_AGENT =
    "org.freedesktop.NetworkManager.SecretAgent";
constexpr const char *DBUS_INTERFACE_NETWORK_MANAGER =
    "org.freedesktop.NetworkManager";
constexpr const char *DBUS_OBJECT_PATH_NETWORK_MANAGER =
    "/org/freedesktop/NetworkManager";

// SNI (StatusNotifierItem) protocol constants
constexpr const char *SNI_WATCHER_INTERFACE = "org.kde.StatusNotifierWatcher";
constexpr const char *SNI_WATCHER_BUS_NAME = "org.kde.StatusNotifierWatcher";
constexpr const char *SNI_WATCHER_OBJECT_PATH = "/StatusNotifierWatcher";
constexpr const char *SNI_ITEM_INTERFACE = "org.kde.StatusNotifierItem";
constexpr const char *SNI_ITEM_OBJECT_PATH = "/StatusNotifierItem";
constexpr const char *SNI_ITEM_BUS_NAME_PREFIX = "org.kde.StatusNotifierItem-";
constexpr const char *SNI_SIGNAL_ITEM_REGISTERED =
    "StatusNotifierItemRegistered";
constexpr const char *SNI_SIGNAL_ITEM_UNREGISTERED =
    "StatusNotifierItemUnregistered";
constexpr const char *DBUS_INTERFACE_DBUS = "org.freedesktop.DBus";
constexpr const char *DBUS_SIGNAL_NAME_OWNER_CHANGED = "NameOwnerChanged";
constexpr const char *DBUSMENU_INTERFACE = "com.canonical.dbusmenu";
