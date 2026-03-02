/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
*/

#pragma once

#include <gio/gio.h>
#include <glib.h>
#include <memory>

// Custom deleters for GLib types that need special cleanup sequences or are
// not covered by g_autoptr().  Local variables use g_autoptr / g_autofree.

// GDBusNodeInfo uses its own ref-counting (not GObject).
struct GNodeInfoDeleter {
  void operator()(GDBusNodeInfo *n) { g_dbus_node_info_unref(n); }
};
using GNodeInfoPtr = std::unique_ptr<GDBusNodeInfo, GNodeInfoDeleter>;

// GDBusConnection must be close()'d before unref().
struct GConnDeleter {
  void operator()(GDBusConnection *c) {
    if (!c)
      return;
    if (!g_dbus_connection_is_closed(c)) {
      GError *err = nullptr;
      g_dbus_connection_close_sync(c, nullptr, &err);
      g_clear_error(&err);
    }
    g_object_unref(c);
  }
};
using GConnPtr = std::unique_ptr<GDBusConnection, GConnDeleter>;

// GDBusMethodInvocation ref-counted via GObject.
struct GInvocationDeleter {
  void operator()(GDBusMethodInvocation *i) { g_object_unref(i); }
};
using GInvocationPtr =
    std::unique_ptr<GDBusMethodInvocation, GInvocationDeleter>;

// GVariant — use this for owned members (locals use g_autoptr(GVariant)).
struct GVariantDeleter {
  void operator()(GVariant *v) { g_variant_unref(v); }
};
using GVariantPtr = std::unique_ptr<GVariant, GVariantDeleter>;

// GDestroyNotify adapter: typed C++ delete as a GLib destroy callback.
// Usage: g_hash_table_new_full(..., glib_delete<SniItem>)
//        g_dbus_connection_register_object(..., glib_delete<SniForwardContext>,
//        ...)
template <typename T> void glib_delete(gpointer p) {
  delete static_cast<T *>(p);
}
