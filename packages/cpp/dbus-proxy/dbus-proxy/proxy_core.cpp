/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "dbus_proxy.h"
#include <gio/gio.h>
#include <glib-unix.h>
#include <glib.h>
#include <string.h>

/* forward decl for free function used by hash table init */
void free_proxied_object(gpointer data);

gboolean init_proxy_state(const ProxyConfig *config) {
  proxy_state = g_new0(ProxyState, 1);
  if (!proxy_state) {
    log_error("Failed to allocate ProxyState");
    return FALSE;
  }
  g_rw_lock_init(&proxy_state->rw_lock);
  proxy_state->config = *config;
  proxy_state->registered_objects =
      g_hash_table_new_full(g_direct_hash, g_direct_equal, nullptr, g_free);
  proxy_state->signal_subscriptions =
      g_hash_table_new_full(g_direct_hash, g_direct_equal, nullptr, g_free);
  proxy_state->catch_all_subscription_id = 0;
  proxy_state->catch_interfaces_added_subscription_id = 0;
  proxy_state->catch_interfaces_removed_subscription_id = 0;
  proxy_state->proxied_objects = g_hash_table_new_full(
      g_str_hash, g_str_equal, g_free, free_proxied_object);
  proxy_state->node_info_cache = g_hash_table_new_full(
      g_str_hash, g_str_equal, g_free, (GDestroyNotify)g_dbus_node_info_unref);

  // Set up signal handlers
  proxy_state->sigint_source_id =
      g_unix_signal_add(SIGINT, signal_handler, GINT_TO_POINTER(SIGINT));
  proxy_state->sigterm_source_id =
      g_unix_signal_add(SIGTERM, signal_handler, GINT_TO_POINTER(SIGTERM));

  return TRUE;
}

gboolean connect_to_buses() {
  GError *error = nullptr;

  proxy_state->source_bus =
      g_bus_get_sync(proxy_state->config.source_bus_type, nullptr, &error);
  if (!proxy_state->source_bus) {
    log_error("Failed to connect to source bus: %s", error->message);
    g_clear_error(&error);
    return FALSE;
  }
  log_info("Connected to source bus (%s)",
           proxy_state->config.source_bus_type == G_BUS_TYPE_SYSTEM
               ? "system"
               : "session");

  proxy_state->target_bus =
      g_bus_get_sync(proxy_state->config.target_bus_type, nullptr, &error);
  if (!proxy_state->target_bus) {
    log_error("Failed to connect to target bus: %s", error->message);
    g_clear_error(&error);
    return FALSE;
  }
  log_info("Connected to target bus (%s)",
           proxy_state->config.target_bus_type == G_BUS_TYPE_SYSTEM
               ? "system"
               : "session");

  return TRUE;
}

gboolean fetch_introspection_data() {
  GError *error = nullptr;

  log_info("Fetching introspection data from %s%s",
           proxy_state->config.source_bus_name,
           proxy_state->config.source_object_path);

  GVariant *xml_variant = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name,
      proxy_state->config.source_object_path, DBUS_INTERFACE_INTROSPECTABLE,
      "Introspect", nullptr, G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE, -1,
      nullptr, &error);

  if (!xml_variant) {
    log_error("Introspection failed: %s", error->message);
    g_clear_error(&error);
    return FALSE;
  }

  const gchar *xml_data;
  g_variant_get(xml_variant, "(&s)", &xml_data);

  log_verbose("Introspection XML received (%zu bytes)", strlen(xml_data));

  proxy_state->introspection_data =
      g_dbus_node_info_new_for_xml(xml_data, &error);
  g_variant_unref(xml_variant);

  if (!proxy_state->introspection_data) {
    log_error("Failed to parse introspection XML: %s", error->message);
    g_clear_error(&error);
    return FALSE;
  }

  log_info("Introspection data parsed successfully");
  return TRUE;
}

GDBusNodeInfo *introspect_node(GDBusConnection *conn, const gchar *bus_name,
                               const gchar *object_path) {
  GVariant *reply;
  GError *error = NULL;
  const gchar *xml;
  GDBusNodeInfo *node_info;

  reply = g_dbus_connection_call_sync(
      conn, bus_name, object_path, DBUS_INTERFACE_INTROSPECTABLE, "Introspect",
      NULL, G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE, -1, NULL, &error);

  if (!reply) {
    g_clear_error(&error);
    return NULL;
  }

  g_variant_get(reply, "(&s)", &xml);

  node_info = g_dbus_node_info_new_for_xml(xml, &error);
  g_variant_unref(reply);

  if (!node_info) {
    log_error("Failed to parse introspection XML: %s",
              error ? error->message : "Unknown");
    g_clear_error(&error);
    return NULL;
  }

  return node_info; /* caller owns a ref */
}

gboolean proxy_object_manager_objects(gchar *object_manager_path) {
  GError *error = NULL;

  GVariant *result = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name,
      object_manager_path, "org.freedesktop.DBus.ObjectManager",
      "GetManagedObjects",
      NULL,                              // No parameters
      G_VARIANT_TYPE("(a{oa{sa{sv}}})"), // Expected return type
      G_DBUS_CALL_FLAGS_NONE,
      -1, // Default timeout
      NULL, &error);

  if (error) {
    log_error("Error: %s", error->message);
    g_clear_error(&error);
    return FALSE;
  }

  g_rw_lock_writer_lock(&proxy_state->rw_lock);
  log_info("=== ObjectManager Managed Objects ===");

  GVariantIter *objects_iter = NULL;
  g_variant_get(result, "(a{oa{sa{sv}}})", &objects_iter);

  const gchar *object_path;
  GVariant *interfaces_var = NULL;

  while (g_variant_iter_next(objects_iter, "{&o@a{sa{sv}}}", &object_path,
                             &interfaces_var)) {
    log_info("Object: %s", object_path);

    GDBusNodeInfo *node_info =
        introspect_node(proxy_state->source_bus,
                        proxy_state->config.source_bus_name, object_path);
    if (node_info) {
      proxy_single_object(object_path, node_info, FALSE);
      g_dbus_node_info_unref(node_info);
    }
    g_variant_unref(interfaces_var);
  }

  g_variant_iter_free(objects_iter);
  g_variant_unref(result);

  log_info("=== End ===");
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  return TRUE;
}

gboolean discover_and_proxy_object_tree(const gchar *base_path,
                                        gchar **foundObjectManagerPath,
                                        gboolean need_lock) {
  GError *error = nullptr;
  GDBusNodeInfo *node_info = nullptr;
  gboolean success = FALSE;

  log_info("Discovering object tree starting from: %s", base_path);

  GVariant *xml_variant = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name, base_path,
      DBUS_INTERFACE_INTROSPECTABLE, "Introspect", nullptr,
      G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE,
      10000, // 10 second timeout
      nullptr, &error);

  if (!xml_variant) {
    if (error && error->domain == G_DBUS_ERROR &&
        error->code == G_DBUS_ERROR_UNKNOWN_OBJECT) {
      log_verbose("Object %s does not exist, skipping", base_path);
    } else {
      log_verbose("Could not introspect %s: %s", base_path,
                  error ? error->message : "Unknown error");
    }
    g_clear_error(&error);
    return TRUE; // Continue with other objects
  }

  const gchar *xml_data;
  g_variant_get(xml_variant, "(&s)", &xml_data);

  log_verbose("Introspection XML for %s (%zu bytes)", base_path,
              strlen(xml_data));

  node_info = g_dbus_node_info_new_for_xml(xml_data, &error);
  g_variant_unref(xml_variant);

  if (!node_info) {
    log_error("Failed to parse introspection XML for %s: %s", base_path,
              error ? error->message : "Unknown");
    g_clear_error(&error);
    return FALSE;
  }

  if (proxy_state->config.verbose && node_info->interfaces) {
    for (int i = 0; node_info->interfaces[i]; i++) {
      log_verbose("Found interface: %s", node_info->interfaces[i]->name);
      if (foundObjectManagerPath &&
          g_strcmp0(node_info->interfaces[i]->name,
                    "org.freedesktop.DBus.ObjectManager") == 0) {
        *foundObjectManagerPath = g_strdup(base_path);
        log_info("ObjectManager found at: %s", *foundObjectManagerPath);
        if (!proxy_single_object(*foundObjectManagerPath, node_info, FALSE)) {
          g_dbus_node_info_unref(node_info);
          return FALSE;
        }
        gboolean result = proxy_object_manager_objects(*foundObjectManagerPath);
        g_dbus_node_info_unref(node_info);
        return result;
      }
    }
  }

  if (proxy_state->config.verbose && node_info->nodes) {
    for (int i = 0; node_info->nodes[i]; i++) {
      const gchar *child_name = node_info->nodes[i]->path;
      log_verbose("Found child node: %s",
                  child_name ? child_name : "(unnamed)");
    }
  }

  if (need_lock) {
    g_rw_lock_writer_lock(&proxy_state->rw_lock);
  }

  if (!proxy_single_object(base_path, node_info, FALSE)) {
    success = FALSE;
    goto cleanup;
  }

  if (need_lock) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  }

  if (node_info->nodes) {
    for (int i = 0; node_info->nodes[i]; i++) {
      const gchar *child_name = node_info->nodes[i]->path;

      if (!child_name || child_name[0] == '\0') {
        log_verbose("Skipping unnamed child node");
        continue;
      }

      gchar *child_path;
      if (g_str_has_suffix(base_path, "/")) {
        child_path = g_strdup_printf("%s%s", base_path, child_name);
      } else {
        child_path = g_strdup_printf("%s/%s", base_path, child_name);
      }

      log_verbose("Recursively processing child: %s", child_path);

      discover_and_proxy_object_tree(child_path, NULL,
                                     TRUE); // Need lock for recursive calls
      g_free(child_path);
    }
  }

  success = TRUE;

  g_dbus_node_info_unref(node_info);
  return success;

cleanup:
  if (need_lock) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  }
  g_dbus_node_info_unref(node_info);
  return success;
}

void free_proxied_object(gpointer data) {
  ProxiedObject *obj = static_cast<ProxiedObject *>(data);
  if (!obj)
    return;

  g_free(obj->object_path);
  if (obj->node_info) {
    g_dbus_node_info_unref(obj->node_info);
  }
  if (obj->registration_ids) {
    g_hash_table_destroy(obj->registration_ids);
  }
  g_free(obj);
}

gboolean proxy_single_object(const gchar *object_path, GDBusNodeInfo *node_info,
                             gboolean need_lock) {
  if (!object_path || !node_info) {
    log_error("Invalid parameters to proxy_single_object");
    return FALSE;
  }

  if (!node_info->interfaces || !node_info->interfaces[0]) {
    log_verbose("Object %s has no interfaces, skipping", object_path);
    return TRUE;
  }

  guint interface_count = 0;
  for (int i = 0; node_info->interfaces[i]; i++) {
    if (!g_strv_contains(standard_interfaces, node_info->interfaces[i]->name)) {
      interface_count++;
    }
  }

  if (interface_count == 0) {
    log_verbose("Object %s has only standard interfaces, skipping",
                object_path);
    return TRUE;
  }

  log_info("Proxying object %s (%u custom interface%s)", object_path,
           interface_count, interface_count == 1 ? "" : "s");

  if (need_lock) {
    g_rw_lock_writer_lock(&proxy_state->rw_lock);
  }

  if (g_hash_table_contains(proxy_state->proxied_objects, object_path)) {
    log_verbose("Object %s is already proxied", object_path);
    if (need_lock) {
      g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    }
    return TRUE;
  }

  ProxiedObject *proxied_obj = g_new0(ProxiedObject, 1);
  proxied_obj->object_path = g_strdup(object_path);
  proxied_obj->node_info = g_dbus_node_info_ref(node_info);
  proxied_obj->registration_ids =
      g_hash_table_new_full(g_str_hash, g_str_equal, g_free, nullptr);

  static const GDBusInterfaceVTable vtable = {.method_call =
                                                  handle_method_call_generic,
                                              .get_property = nullptr,
                                              .set_property = nullptr,
                                              .padding = {nullptr}};

  guint registered_count = 0;

  for (int i = 0; node_info->interfaces[i]; i++) {
    GDBusInterfaceInfo *iface = node_info->interfaces[i];

    if (g_strv_contains(standard_interfaces, iface->name)) {
      continue;
    }

    log_verbose("Registering interface %s on object %s", iface->name,
                object_path);

    GError *error = nullptr;
    guint registration_id = g_dbus_connection_register_object(
        proxy_state->target_bus, object_path, iface, &vtable,
        g_strdup(object_path), // Pass object path as user_data for forwarding
        g_free, &error);

    if (registration_id == 0) {
      log_error("Failed to register interface %s on %s: %s", iface->name,
                object_path, error ? error->message : "Unknown error");
      if (error)
        g_clear_error(&error);
      continue; // Try other interfaces
    }

    registered_count++;

    g_hash_table_insert(proxied_obj->registration_ids, g_strdup(iface->name),
                        GUINT_TO_POINTER(registration_id));

    g_hash_table_insert(proxy_state->registered_objects,
                        GUINT_TO_POINTER(registration_id),
                        g_strdup_printf("%s:%s", object_path, iface->name));

    log_verbose("Interface %s registered on %s with reg_id %u", iface->name,
                object_path, registration_id);
  }

  if (registered_count > 0) {
    g_hash_table_insert(proxy_state->proxied_objects, g_strdup(object_path),
                        proxied_obj);

    log_info("Successfully proxied object %s with %u interface%s", object_path,
             registered_count, registered_count == 1 ? "" : "s");
  } else {
    log_verbose("No custom interfaces registered for %s", object_path);
    free_proxied_object(proxied_obj);
  }

  if (need_lock) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  }

  return TRUE;
}

gboolean register_single_interface(const gchar *object_path,
                                   const gchar *interface_name,
                                   ProxiedObject *proxied_obj) {
  if (g_strv_contains(standard_interfaces, interface_name)) {
    return TRUE;
  }

  GError *error = nullptr;
  GVariant *xml_variant = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name, object_path,
      DBUS_INTERFACE_INTROSPECTABLE, "Introspect", nullptr,
      G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, &error);

  if (!xml_variant) {
    log_error("Failed to introspect %s for interface %s: %s", object_path,
              interface_name, error ? error->message : "Unknown");
    if (error)
      g_clear_error(&error);
    return FALSE;
  }

  const gchar *xml_data;
  g_variant_get(xml_variant, "(&s)", &xml_data);

  GDBusNodeInfo *node_info = g_dbus_node_info_new_for_xml(xml_data, &error);
  g_variant_unref(xml_variant);

  if (!node_info) {
    log_error("Failed to parse introspection XML: %s",
              error ? error->message : "Unknown");
    g_clear_error(&error);
    return FALSE;
  }

  GDBusInterfaceInfo *iface_info =
      g_dbus_node_info_lookup_interface(node_info, interface_name);

  if (!iface_info) {
    log_error("Interface %s not found in introspection data", interface_name);
    g_dbus_node_info_unref(node_info);
    return FALSE;
  }

  static const GDBusInterfaceVTable vtable = {.method_call =
                                                  handle_method_call_generic,
                                              .get_property = nullptr,
                                              .set_property = nullptr,
                                              .padding = {nullptr}};

  guint registration_id = g_dbus_connection_register_object(
      proxy_state->target_bus, object_path, iface_info, &vtable,
      g_strdup(object_path), g_free, &error);

  if (registration_id == 0) {
    log_error("Failed to register interface %s on %s: %s", interface_name,
              object_path, error ? error->message : "Unknown");
    g_clear_error(&error);
    g_dbus_node_info_unref(node_info);
    return FALSE;
  }

  gchar *cache_key = g_strdup_printf("%s:%s", object_path, interface_name);
  g_hash_table_insert(proxy_state->node_info_cache, cache_key,
                      node_info); // Don't unref - stored in cache

  g_hash_table_insert(proxied_obj->registration_ids, g_strdup(interface_name),
                      GUINT_TO_POINTER(registration_id));

  g_hash_table_insert(proxy_state->registered_objects,
                      GUINT_TO_POINTER(registration_id),
                      g_strdup_printf("%s:%s", object_path, interface_name));

  log_info("Successfully registered interface %s on %s (ID: %u)",
           interface_name, object_path, registration_id);

  return TRUE;
}

gboolean setup_signal_forwarding() {
  log_info("Setting up signal forwarding");

  g_rw_lock_writer_lock(&proxy_state->rw_lock);

  proxy_state->catch_all_subscription_id = g_dbus_connection_signal_subscribe(
      proxy_state->source_bus,
      proxy_state->config.source_bus_name, // sender (our source service)
      nullptr,                             // interface_name (all interfaces)
      nullptr,                             // method: member (all signals)
      nullptr, // object_path (all paths - we filter in callback)
      nullptr, // arg0 (no filtering)
      G_DBUS_SIGNAL_FLAGS_NONE, on_signal_received_catchall, nullptr, nullptr);

  if (proxy_state->catch_all_subscription_id == 0) {
    log_error("Failed to set up catch-all signal subscription");
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    return FALSE;
  }
  g_hash_table_insert(proxy_state->signal_subscriptions,
                      GUINT_TO_POINTER(proxy_state->catch_all_subscription_id),
                      g_strdup("catch-all"));
  log_info("Catch-all signal subscription established (ID: %u)",
           proxy_state->catch_all_subscription_id);

  proxy_state->catch_interfaces_added_subscription_id =
      g_dbus_connection_signal_subscribe(
          proxy_state->source_bus, proxy_state->config.source_bus_name,
          DBUS_INTERFACE_OBJECT_MANAGER, // interface
          DBUS_SIGNAL_INTERFACES_ADDED,  // method: New objects appear
          nullptr,                       // Any object path
          nullptr,                       // No arg0 filtering
          G_DBUS_SIGNAL_FLAGS_NONE, on_interfaces_added, nullptr, nullptr);

  if (proxy_state->catch_interfaces_added_subscription_id == 0) {
    log_error("Failed to set up InterfacesAdded signal subscription");
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    return FALSE;
  }
  g_hash_table_insert(
      proxy_state->signal_subscriptions,
      GUINT_TO_POINTER(proxy_state->catch_interfaces_added_subscription_id),
      g_strdup(DBUS_SIGNAL_INTERFACES_ADDED));
  log_info("InterfacesAdded signal subscription established (ID: %u)",
           proxy_state->catch_interfaces_added_subscription_id);

  proxy_state->catch_interfaces_removed_subscription_id =
      g_dbus_connection_signal_subscribe(
          proxy_state->source_bus, proxy_state->config.source_bus_name,
          DBUS_INTERFACE_OBJECT_MANAGER,  // interface
          DBUS_SIGNAL_INTERFACES_REMOVED, // method: Objects removed
          nullptr,                        // Any object path
          nullptr,                        // No arg0 filtering
          G_DBUS_SIGNAL_FLAGS_NONE, on_interfaces_removed, nullptr, nullptr);

  if (proxy_state->catch_interfaces_removed_subscription_id == 0) {
    log_error("Failed to set up InterfacesRemoved signal subscription");
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    return FALSE;
  }
  g_hash_table_insert(
      proxy_state->signal_subscriptions,
      GUINT_TO_POINTER(proxy_state->catch_interfaces_removed_subscription_id),
      g_strdup(DBUS_SIGNAL_INTERFACES_REMOVED));
  log_info("InterfacesRemoved signal subscription established (ID: %u)",
           proxy_state->catch_interfaces_removed_subscription_id);
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  return TRUE;
}

gboolean setup_proxy_interfaces() {
  log_info("Setting up proxy interfaces - discovering full object tree");
  gchar *object_manager_path = nullptr;

  if (!setup_signal_forwarding()) {
    return FALSE;
  }

  if (!discover_and_proxy_object_tree("/", &object_manager_path, TRUE)) {
    log_error("Failed to discover and proxy D-Bus daemon interface");
    return FALSE;
  }

  if (object_manager_path) {
    log_info("ObjectManager interface proxied at: %s", object_manager_path);
    g_free(object_manager_path);
  }

  log_info("Object tree proxying complete - %u objects proxied",
           g_hash_table_size(proxy_state->proxied_objects));

  return TRUE;
}

void on_bus_acquired_for_owner(GDBusConnection *connection, const gchar *name,
                               gpointer user_data G_GNUC_UNUSED) {
  log_info("Bus acquired for name: %s", name ? name : "(none)");
  if (!proxy_state)
    return;

  if (proxy_state->target_bus) {
    g_object_unref(proxy_state->target_bus);
    proxy_state->target_bus = nullptr;
  }
  proxy_state->target_bus = g_object_ref(connection);

  if (!setup_proxy_interfaces()) {
    log_error("Failed to set up interfaces on target bus");
    if (proxy_state->name_owner_watch_id) {
      g_bus_unown_name(proxy_state->name_owner_watch_id);
      proxy_state->name_owner_watch_id = 0;
    }
  }
}

void on_name_acquired_log(G_GNUC_UNUSED GDBusConnection *conn,
                          const gchar *name, gpointer user_data G_GNUC_UNUSED) {
  log_info("Name successfully acquired: %s", name);
}

void on_name_lost_log(G_GNUC_UNUSED GDBusConnection *conn, const gchar *name,
                      gpointer user_data G_GNUC_UNUSED) {
  log_error("Name lost or failed to acquire: %s", name);
}

void cleanup_proxy_state() {
  if (!proxy_state)
    return;

  if (proxy_state->registered_objects) {
    GHashTableIter iter;
    gpointer key, value;
    g_hash_table_iter_init(&iter, proxy_state->registered_objects);
    while (g_hash_table_iter_next(&iter, &key, &value)) {
      g_dbus_connection_unregister_object(proxy_state->target_bus,
                                          GPOINTER_TO_UINT(key));
    }
    g_hash_table_destroy(proxy_state->registered_objects);
  }
  if (proxy_state->node_info_cache) {
    g_hash_table_destroy(proxy_state->node_info_cache);
  }
  if (proxy_state->catch_all_subscription_id && proxy_state->source_bus) {
    g_dbus_connection_signal_unsubscribe(
        proxy_state->source_bus, proxy_state->catch_all_subscription_id);
    proxy_state->catch_all_subscription_id = 0;
  }
  if (proxy_state->catch_interfaces_added_subscription_id &&
      proxy_state->source_bus) {
    g_dbus_connection_signal_unsubscribe(
        proxy_state->source_bus,
        proxy_state->catch_interfaces_added_subscription_id);
    proxy_state->catch_interfaces_added_subscription_id = 0;
  }
  if (proxy_state->catch_interfaces_removed_subscription_id &&
      proxy_state->source_bus) {
    g_dbus_connection_signal_unsubscribe(
        proxy_state->source_bus,
        proxy_state->catch_interfaces_removed_subscription_id);
    proxy_state->catch_interfaces_removed_subscription_id = 0;
  }
  if (proxy_state->signal_subscriptions) {
    GHashTableIter iter;
    gpointer key, value;
    g_hash_table_iter_init(&iter, proxy_state->signal_subscriptions);
    while (g_hash_table_iter_next(&iter, &key, &value)) {
      g_dbus_connection_signal_unsubscribe(proxy_state->source_bus,
                                           GPOINTER_TO_UINT(key));
    }
    g_hash_table_destroy(proxy_state->signal_subscriptions);
  }
  if (proxy_state->proxied_objects) {
    g_hash_table_destroy(proxy_state->proxied_objects);
  }

  unregister_all_agent_registrations();
  if (proxy_state->senders_registry) {
    g_hash_table_destroy(proxy_state->senders_registry);
    proxy_state->senders_registry = nullptr;
  }

  if (proxy_state->introspection_data) {
    g_dbus_node_info_unref(proxy_state->introspection_data);
  }

  g_dbus_connection_flush_sync(proxy_state->source_bus, NULL, NULL);
  g_dbus_connection_flush_sync(proxy_state->target_bus, NULL, NULL);

  g_dbus_connection_close_sync(proxy_state->source_bus, NULL, NULL);
  g_dbus_connection_close_sync(proxy_state->target_bus, NULL, NULL);

  if (proxy_state->source_bus) {
    g_object_unref(proxy_state->source_bus);
  }

  if (proxy_state->target_bus) {
    g_object_unref(proxy_state->target_bus);
  }

  g_free(proxy_state->config.source_bus_name);
  g_free(proxy_state->config.proxy_bus_name);
  g_free(proxy_state->config.source_object_path);
  if (proxy_state->name_owner_watch_id) {
    g_bus_unown_name(proxy_state->name_owner_watch_id);
    proxy_state->name_owner_watch_id = 0;
  }

  g_rw_lock_clear(&proxy_state->rw_lock);
  g_free(proxy_state);
  proxy_state = nullptr;
}

GBusType parse_bus_type(const gchar *bus_str) {
  if (g_strcmp0(bus_str, "system") == 0) {
    return G_BUS_TYPE_SYSTEM;
  } else if (g_strcmp0(bus_str, "session") == 0) {
    return G_BUS_TYPE_SESSION;
  }
  return G_BUS_TYPE_SYSTEM; // Default
}

void validateProxyConfigOrExit(const ProxyConfig *config) {
  if (!config->source_bus_name || config->source_bus_name[0] == '\0') {
    log_error("Error: source_bus_name is required!");
    exit(EXIT_FAILURE);
  }
  if (!config->source_object_path || config->source_object_path[0] == '\0') {
    log_error("Error: source_object_path is required!");
    exit(EXIT_FAILURE);
  }
  if (!config->proxy_bus_name || config->proxy_bus_name[0] == '\0') {
    log_error("Error: proxy_bus_name is required!");
    exit(EXIT_FAILURE);
  }
}
