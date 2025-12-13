/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

/*
 * Cross-Bus GDBus Proxy that:
 * 1. Connects to two different D-Bus buses (source and target).
 * 2. Fetches introspection data from source service on source bus.
 * 3. Exposes that interface on target bus with proxy name.
 * 4. If works in nm-applet mode, the proxy exposes nm-applet's
 *    interface (Secret Agent) on source bus, as NetworkManager calls it.
 * 5. In nm-applet mode forwards calls from NetworkManager to the real
 *    nm-applet service on the destination bus.
 * 6. Forwards method calls from target bus to source bus.
 * 7. Forwards signals from source bus to target bus.
 * 8. Handles properties synchronization between buses.
 */

#include "./gdbusprivate.h"
#include <gio/gio.h>
#include <glib-unix.h>
#include <glib.h>
#include <glib/gprintf.h>

// Configuration structure
typedef struct {
  gchar *source_bus_name;
  gchar *source_object_path;
  gchar *proxy_bus_name;
  GBusType source_bus_type;
  GBusType target_bus_type;
  gboolean nm_mode;
  gboolean verbose;
  gboolean info;
} ProxyConfig;

// Global state
typedef struct {
  GDBusConnection *source_bus;
  GDBusConnection *target_bus;
  GDBusNodeInfo *introspection_data;
  GHashTable *registered_objects;   // Track registered object IDs
  GHashTable *signal_subscriptions; // Track signal subscription IDs
  ProxyConfig config;
  guint secret_agent_reg_id;
  gchar *client_sender_name;
  guint name_owner_watch_id;
  guint source_service_watch_id;
  guint catch_all_subscription_id;              // For catching all signals
  guint catch_interfaces_added_subscription_id; // For catching InterfacesAdded
  guint catch_interfaces_removed_subscription_id; // For catching
                                                  // InterfacesRemoved
  GHashTable *proxied_objects; // object_path -> ProxiedObject*
  GHashTable *node_info_cache;
  GRWLock rw_lock;
  guint sigint_source_id;
  guint sigterm_source_id;
  GMainLoop *main_loop;
} ProxyState;

// Structure to track proxied objects
typedef struct {
  char *object_path;
  GDBusNodeInfo *node_info;
  GHashTable *registration_ids; // interface_name -> registration_id
} ProxiedObject;
static ProxyState *proxy_state = nullptr;

// Logging functions
static void log_verbose(const char *format, ...) {
  if (proxy_state && !proxy_state->config.verbose)
    return;

  va_list args;
  va_start(args, format);
  g_print("[VERBOSE] ");
  g_vprintf(format, args);
  g_print("\n");
  va_end(args);
}

static void log_error(const char *format, ...) {
  va_list args;
  va_start(args, format);
  g_printerr("[ERROR] ");
  g_vfprintf(stderr, format, args);
  g_printerr("\n");
  va_end(args);
}

static void log_info(const char *format, ...) {
  if (proxy_state && !proxy_state->config.info)
    return;

  va_list args;
  va_start(args, format);
  g_print("[INFO] ");
  g_vprintf(format, args);
  g_print("\n");
  va_end(args);
}

static const char *standard_interfaces[] = {DBUS_INTERFACE_INTROSPECTABLE,
                                            DBUS_INTERFACE_PEER,
                                            DBUS_INTERFACE_PROPERTIES, nullptr};

static gboolean proxy_single_object(const char *object_path,
                                    GDBusNodeInfo *node_info,
                                    gboolean need_lock);

static gboolean register_single_interface(const char *object_path,
                                          const char *interface_name,
                                          ProxiedObject *proxied_obj);

static gboolean signal_handler(void *user_data);

// Free function for ProxiedObject
static void free_proxied_object(gpointer data) {
  ProxiedObject *obj_to_free = static_cast<ProxiedObject *>(data);
  if (!obj_to_free)
    return;

  g_free(obj_to_free->object_path);
  if (obj_to_free->node_info) {
    g_dbus_node_info_unref(obj_to_free->node_info);
  }
  if (obj_to_free->registration_ids) {
    g_hash_table_destroy(obj_to_free->registration_ids);
  }
  g_free(obj_to_free);
}

// Recursively discover and proxy all objects starting from a base path
static gboolean discover_and_proxy_object_tree(const char *base_path,
                                               gboolean need_lock) {
  GError *error = nullptr;
  GDBusNodeInfo *node_info = nullptr;
  gboolean success = FALSE;

  log_info("Discovering object tree starting from: %s", base_path);

  // Get introspection data for this path
  GVariant *xml_variant = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name, base_path,
      DBUS_INTERFACE_INTROSPECTABLE, "Introspect", nullptr,
      G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE,
      10000, // 10 second timeout
      nullptr, &error);

  if (!xml_variant) {
    // Some objects might not be introspectable, that's ok
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

  const char *xml_data;
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

  // Log what we found
  if (proxy_state->config.verbose && node_info->interfaces) {
    for (int i = 0; node_info->interfaces[i]; i++) {
      log_verbose("Found interface: %s", node_info->interfaces[i]->name);
    }
  }

  if (proxy_state->config.verbose && node_info->nodes) {
    for (int i = 0; node_info->nodes[i]; i++) {
      const char *child_name = node_info->nodes[i]->path;
      log_verbose("Found child node: %s",
                  child_name ? child_name : "(unnamed)");
    }
  }

  // Acquire lock if needed
  if (need_lock) {
    g_rw_lock_writer_lock(&proxy_state->rw_lock);
  }

  // Proxy this object if it has interfaces
  if (!proxy_single_object(base_path, node_info, FALSE)) {
    // proxy_single_object failed
    success = FALSE;
    goto cleanup;
  }

  // Release lock before recursion to avoid holding it during slow operations
  if (need_lock) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  }

  // Recursively handle child nodes
  if (node_info->nodes) {
    for (int i = 0; node_info->nodes[i]; i++) {
      const char *child_name = node_info->nodes[i]->path;

      if (!child_name || child_name[0] == '\0') {
        log_verbose("Skipping unnamed child node");
        continue;
      }

      // Build full child path
      char *child_path;
      if (g_str_has_suffix(base_path, "/")) {
        child_path = g_strdup_printf("%s%s", base_path, child_name);
      } else {
        child_path = g_strdup_printf("%s/%s", base_path, child_name);
      }

      log_verbose("Recursively processing child: %s", child_path);

      // Recurse into child (don't fail if child fails)
      discover_and_proxy_object_tree(child_path,
                                     TRUE); // Need lock for recursive calls
      g_free(child_path);
    }
  }

  success = TRUE;

  // No need to cleanup lock here since we already released it before recursion
  g_dbus_node_info_unref(node_info);
  return success;

cleanup:
  if (need_lock) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  }
  g_dbus_node_info_unref(node_info);
  return success;
}

struct MethodCallContext {
  GDBusMethodInvocation *invocation;
  char *forward_bus_name;
};

static void method_call_reply_callback(GObject *source, GAsyncResult *res,
                                       gpointer user_data) {
  MethodCallContext *context = static_cast<MethodCallContext *>(user_data);
  GError *error = nullptr;

  GVariant *result =
      g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), res, &error);

  if (result) {
    log_verbose("Method call successful, returning result");
    g_dbus_method_invocation_return_value(context->invocation, result);
    g_variant_unref(result);
  } else {
    log_error("Method call failed: %s",
              error ? error->message : "Unknown error");
    g_dbus_method_invocation_return_gerror(context->invocation, error);
    g_clear_error(&error);
  }

  g_object_unref(context->invocation);
  g_free(context->forward_bus_name);
  g_free(context);
}

static void handle_method_call_generic(
    GDBusConnection *connection, const char *sender, const char *object_path,
    const char *interface_name, const char *method_name, GVariant *parameters,
    GDBusMethodInvocation *invocation, gpointer user_data) {

  const char *target_object_path = static_cast<const char *>(user_data);

  log_verbose("Method call: %s.%s on %s from %s (forwarding to %s)",
              interface_name, method_name, object_path, sender,
              target_object_path);

  GDBusConnection *forward_bus = nullptr;
  char *forward_bus_name = nullptr;

  if (connection == proxy_state->target_bus) {
    forward_bus = proxy_state->source_bus;
    forward_bus_name = g_strdup(proxy_state->config.source_bus_name);

    if (proxy_state->config.nm_mode) {
      g_rw_lock_writer_lock(&proxy_state->rw_lock);
      if (!proxy_state->client_sender_name ||
          g_strcmp0(proxy_state->client_sender_name, sender) != 0) {
        g_free(proxy_state->client_sender_name);
        proxy_state->client_sender_name = g_strdup(sender);
        log_info("Remembering client sender name: %s", sender);
      }
      g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    }
  } else if (connection == proxy_state->source_bus) {
    forward_bus = proxy_state->target_bus;

    g_rw_lock_reader_lock(&proxy_state->rw_lock);
    if (!proxy_state->client_sender_name) {
      g_rw_lock_reader_unlock(&proxy_state->rw_lock);
      log_error("No client sender registered, cannot forward method call");
      g_dbus_method_invocation_return_error(invocation, G_DBUS_ERROR,
                                            G_DBUS_ERROR_FAILED,
                                            "No client registered with proxy");
      return;
    }
    forward_bus_name = g_strdup(proxy_state->client_sender_name);
    g_rw_lock_reader_unlock(&proxy_state->rw_lock);

    log_verbose("Forwarding back to client: %s", forward_bus_name);

  } else {
    log_error("Method call from unknown connection %p", connection);
    g_dbus_method_invocation_return_error(
        invocation, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
        "Internal proxy error: unknown connection");
    return;
  }

  MethodCallContext *context = g_new0(MethodCallContext, 1);
  context->invocation = invocation;
  context->forward_bus_name = forward_bus_name;

  g_object_ref(invocation);

  g_dbus_connection_call(forward_bus, forward_bus_name, target_object_path,
                         interface_name, method_name, parameters, nullptr,
                         G_DBUS_CALL_FLAGS_NONE, -1, nullptr,
                         method_call_reply_callback, context);
}

// Generic property handlers that work for any object path
static GVariant *handle_get_property_generic(
    G_GNUC_UNUSED GDBusConnection *connection, const char *sender,
    const char *object_path, const char *interface_name,
    const char *property_name, GError **error, gpointer user_data) {
  const char *target_object_path = static_cast<const char *>(user_data);

  log_verbose("Property get: %s.%s on %s from %s (forwarding to %s)",
              interface_name, property_name, object_path, sender,
              target_object_path);

  GVariant *result = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name,
      target_object_path, DBUS_INTERFACE_PROPERTIES, "Get",
      g_variant_new("(ss)", interface_name, property_name),
      G_VARIANT_TYPE("(v)"), G_DBUS_CALL_FLAGS_NONE, -1, nullptr, error);

  if (result) {
    GVariant *value;
    g_variant_get(result, "(v)", &value);
    g_variant_unref(result);
    log_verbose("Property get successful");
    return value; // Caller will unref
  }

  log_error("Property get failed: %s",
            error && *error ? (*error)->message : "Unknown error");
  return nullptr;
}

static gboolean
handle_set_property_generic(G_GNUC_UNUSED GDBusConnection *connection,
                            const char *sender, const char *object_path,
                            const char *interface_name,
                            const char *property_name, GVariant *value,
                            GError **error, gpointer user_data) {
  const char *target_object_path = static_cast<const char *>(user_data);

  log_verbose("Property set: %s.%s on %s from %s (forwarding to %s)",
              interface_name, property_name, object_path, sender,
              target_object_path);

  GVariant *result = g_dbus_connection_call_sync(
      proxy_state->source_bus, proxy_state->config.source_bus_name,
      target_object_path, DBUS_INTERFACE_PROPERTIES, "Set",
      g_variant_new("(ssv)", interface_name, property_name, value), nullptr,
      G_DBUS_CALL_FLAGS_NONE, -1, nullptr, error);

  if (result) {
    g_variant_unref(result);
    return TRUE;
  }

  log_error("Property set failed: %s",
            error && *error ? (*error)->message : "Unknown error");
  return FALSE;
}

// Proxy a single object with all its interfaces
static gboolean proxy_single_object(const char *object_path,
                                    GDBusNodeInfo *node_info,
                                    gboolean need_lock) {
  // Validate parameters
  if (!object_path || !node_info) {
    log_error("Invalid parameters to proxy_single_object");
    return FALSE;
  }

  // Early validation before locking
  if (!node_info->interfaces || !node_info->interfaces[0]) {
    log_verbose("Object %s has no interfaces, skipping", object_path);
    return TRUE;
  }

  // Count non-standard interfaces
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

  // Check for duplicate
  if (g_hash_table_contains(proxy_state->proxied_objects, object_path)) {
    log_verbose("Object %s is already proxied", object_path);
    if (need_lock) {
      g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    }
    return TRUE;
  }

  // Create proxied object structure
  ProxiedObject *proxied_obj = g_new0(ProxiedObject, 1);
  proxied_obj->object_path = g_strdup(object_path);
  proxied_obj->node_info = g_dbus_node_info_ref(node_info);
  proxied_obj->registration_ids =
      g_hash_table_new_full(g_str_hash, g_str_equal, g_free, nullptr);

  // Static vtable (shared across all calls)
  static const GDBusInterfaceVTable vtable = {
      .method_call = handle_method_call_generic,
      .get_property = handle_get_property_generic,
      .set_property = handle_set_property_generic,
      .padding = {nullptr}};

  guint registered_count = 0;

  // Register each interface (except standard ones)
  for (int i = 0; node_info->interfaces[i]; i++) {
    GDBusInterfaceInfo *iface = node_info->interfaces[i];

    // Skip standard D-Bus interfaces
    if (g_strv_contains(standard_interfaces, iface->name)) {
      log_verbose("Skipping standard interface: %s", iface->name);
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

    // Store registration ID
    g_hash_table_insert(proxied_obj->registration_ids, g_strdup(iface->name),
                        GUINT_TO_POINTER(registration_id));

    // Add to global registry for cleanup
    g_hash_table_insert(proxy_state->registered_objects,
                        GUINT_TO_POINTER(registration_id),
                        g_strdup_printf("%s:%s", object_path, iface->name));

    log_verbose("Interface %s registered on %s with ID %u", iface->name,
                object_path, registration_id);
  }

  if (registered_count > 0) {
    // Store the proxied object
    g_hash_table_insert(proxy_state->proxied_objects, g_strdup(object_path),
                        proxied_obj);

    log_info("Successfully proxied object %s with %u interface%s", object_path,
             registered_count, registered_count == 1 ? "" : "s");
  } else {
    // No interfaces registered, clean up
    log_verbose("No custom interfaces registered for %s", object_path);
    free_proxied_object(proxied_obj);
  }

  if (need_lock) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  }

  return TRUE;
}

// Forward signals from source bus to target bus - catch-all version
static void
on_signal_received_catchall(GDBusConnection *connection G_GNUC_UNUSED,
                            const char *sender_name, const char *object_path,
                            const char *interface_name, const char *signal_name,
                            GVariant *parameters,
                            gpointer user_data G_GNUC_UNUSED) {
  // Check if this is a path we're proxying
  g_rw_lock_reader_lock(&proxy_state->rw_lock);
  gboolean is_proxied =
      g_hash_table_contains(proxy_state->proxied_objects, object_path);
  g_rw_lock_reader_unlock(&proxy_state->rw_lock);

  log_verbose("Signal received: %s.%s from %s at %s", interface_name,
              signal_name, sender_name, object_path);

  if (g_strcmp0(signal_name, DBUS_SIGNAL_INTERFACES_ADDED) == 0 &&
      g_strcmp0(interface_name, DBUS_INTERFACE_OBJECT_MANAGER) == 0) {
    log_verbose("Skipping InterfacesAdded in catch-all");
    return;
  }
  // Forward only if it's a proxied object or the D-Bus daemon itself
  if (is_proxied ||
      g_str_has_prefix(object_path, proxy_state->config.source_object_path) ||
      g_strcmp0(object_path, DBUS_OBJECT_PATH_DBUS) == 0) {

    GError *error = nullptr;
    gboolean success = g_dbus_connection_emit_signal(
        proxy_state->target_bus, nullptr, object_path, interface_name,
        signal_name, parameters, &error);

    if (!success) {
      log_error("Failed to forward signal: %s",
                error ? error->message : "Unknown error");
      g_clear_error(&error);
    }
  } else {
    log_error("Signal %s.%s from %s at %s ignored (not proxied)",
              interface_name, signal_name, sender_name, object_path);
  }
}

static void update_object_with_new_interfaces(const char *object_path,
                                              GVariant *interfaces_dict) {
  if (!object_path || !interfaces_dict) {
    log_error("Invalid parameters");
    return;
  }

  g_rw_lock_writer_lock(&proxy_state->rw_lock);

  ProxiedObject *existing_obj = static_cast<ProxiedObject *>(
      g_hash_table_lookup(proxy_state->proxied_objects, object_path));

  if (!existing_obj) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    log_info("Object %s not found, creating new proxy", object_path);
    discover_and_proxy_object_tree(object_path, TRUE);
    return;
  }

  // Iterate through the new interfaces
  GVariantIter iter;
  const char *interface_name;
  GVariant *properties = nullptr;

  g_variant_iter_init(&iter, interfaces_dict);
  while (
      g_variant_iter_next(&iter, "{&s@a{sv}}", &interface_name, &properties)) {
    // Check if this interface is already registered
    if (g_hash_table_contains(existing_obj->registration_ids, interface_name)) {
      log_verbose("Interface %s already registered on %s", interface_name,
                  object_path);
      g_variant_unref(properties);
      continue;
    }

    log_info("Adding new interface %s to object %s", interface_name,
             object_path);

    // Register the new interface
    register_single_interface(object_path, interface_name, existing_obj);

    g_variant_unref(properties);
  }

  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
}

static gboolean register_single_interface(const char *object_path,
                                          const char *interface_name,
                                          ProxiedObject *proxied_obj) {
  // Skip standard interfaces
  if (g_strv_contains(standard_interfaces, interface_name)) {
    log_verbose("Skipping standard interface: %s", interface_name);
    return TRUE;
  }

  // Need to get interface info - introspect the object
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

  const char *xml_data;
  g_variant_get(xml_variant, "(&s)", &xml_data);

  GDBusNodeInfo *node_info = g_dbus_node_info_new_for_xml(xml_data, &error);
  g_variant_unref(xml_variant);

  if (!node_info) {
    log_error("Failed to parse introspection XML: %s",
              error ? error->message : "Unknown");
    g_clear_error(&error);
    return FALSE;
  }

  // Find the specific interface
  GDBusInterfaceInfo *iface_info =
      g_dbus_node_info_lookup_interface(node_info, interface_name);

  if (!iface_info) {
    log_error("Interface %s not found in introspection data", interface_name);
    g_dbus_node_info_unref(node_info);
    return FALSE;
  }

  // Register the interface
  static const GDBusInterfaceVTable vtable = {
      .method_call = handle_method_call_generic,
      .get_property = handle_get_property_generic,
      .set_property = handle_set_property_generic,
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

  // Store node_info in global cache (keeps iface_info alive)
  char *cache_key = g_strdup_printf("%s:%s", object_path, interface_name);
  g_hash_table_insert(proxy_state->node_info_cache, cache_key,
                      node_info); // Don't unref - stored in cache

  // Store registration ID
  g_hash_table_insert(proxied_obj->registration_ids, g_strdup(interface_name),
                      GUINT_TO_POINTER(registration_id));

  // Add to global registry
  g_hash_table_insert(proxy_state->registered_objects,
                      GUINT_TO_POINTER(registration_id),
                      g_strdup_printf("%s:%s", object_path, interface_name));

  log_info("Successfully registered interface %s on %s (ID: %u)",
           interface_name, object_path, registration_id);

  return TRUE;
}

// Forward signals from source bus to target bus - InterfacesAdded handler
static void on_interfaces_added(GDBusConnection *connection G_GNUC_UNUSED,
                                const char *sender_name G_GNUC_UNUSED,
                                const char *object_path,
                                const char *interface_name,
                                const char *signal_name, GVariant *parameters,
                                gpointer user_data G_GNUC_UNUSED) {
  const char *added_object_path;
  GVariant *interfaces_and_properties;

  g_variant_get(parameters, "(&o@a{sa{sv}})", &added_object_path,
                &interfaces_and_properties);

  log_info("InterfacesAdded signal for: %s", added_object_path);

  // Update or create the object with new interfaces
  update_object_with_new_interfaces(added_object_path,
                                    interfaces_and_properties);

  g_variant_unref(interfaces_and_properties);

  // Send signal to the target bus
  GError *error = nullptr;
  gboolean success = g_dbus_connection_emit_signal(
      proxy_state->target_bus, nullptr, object_path, interface_name,
      signal_name, parameters, &error);

  if (!success) {
    log_error("Failed to forward signal: %s",
              error ? error->message : "Unknown error");
    if (error)
      g_clear_error(&error);
  }
}

static void on_interfaces_removed(GDBusConnection *connection G_GNUC_UNUSED,
                                  const char *sender_name G_GNUC_UNUSED,
                                  const char *object_path G_GNUC_UNUSED,
                                  const char *interface_name G_GNUC_UNUSED,
                                  const char *signal_name G_GNUC_UNUSED,
                                  GVariant *parameters,
                                  gpointer user_data G_GNUC_UNUSED) {
  const gchar *removed_object_path;
  gchar **removed_interfaces = nullptr;

  // InterfacesRemoved has signature: (oas)
  // object_path + array of interface names
  g_variant_get(parameters, "(&o^as)", &removed_object_path,
                &removed_interfaces);

  if (!removed_interfaces || removed_interfaces[0] == NULL) {
    log_info("InterfacesRemoved signal with no interfaces for %s",
             removed_object_path);
    g_strfreev(removed_interfaces);
    return;
  }
  // Log what was removed
  if (proxy_state->config.info) {
    char *iface_list = g_strjoinv(", ", removed_interfaces);
    log_info("InterfacesRemoved: %s [%s]", removed_object_path, iface_list);
    g_free(iface_list);
  }

  g_rw_lock_writer_lock(&proxy_state->rw_lock);

  // Look up the proxied object
  ProxiedObject *obj = static_cast<ProxiedObject *>(
      g_hash_table_lookup(proxy_state->proxied_objects, removed_object_path));
  if (!obj) {
    g_rw_lock_writer_unlock(&proxy_state->rw_lock);
    log_verbose("Object %s not in proxy cache, ignoring removal",
                removed_object_path);
    g_strfreev(removed_interfaces);
    return;
  }

  // Unregister only the specific interfaces that were removed
  for (gsize i = 0; removed_interfaces[i] != NULL; i++) {
    const char *iface = removed_interfaces[i];

    guint reg_id =
        GPOINTER_TO_UINT(g_hash_table_lookup(obj->registration_ids, iface));
    if (reg_id == 0) {
      log_verbose("Interface %s on %s was not registered, skipping", iface,
                  removed_object_path);
      continue;
    }
    // Unregister from D-Bus
    gboolean success =
        g_dbus_connection_unregister_object(proxy_state->target_bus, reg_id);
    // Remove from global cache
    char *cache_key = g_strdup_printf("%s:%s", removed_object_path, iface);
    g_hash_table_remove(proxy_state->node_info_cache, cache_key);
    g_free(cache_key);

    if (success) {
      log_verbose("Unregistered interface %s on %s (reg_id=%u)", iface,
                  removed_object_path, reg_id);
    } else {
      log_error("Failed to unregister interface %s on %s (reg_id=%u)", iface,
                removed_object_path, reg_id);
    }
    // Remove from our tracking tables
    g_hash_table_remove(proxy_state->registered_objects,
                        GUINT_TO_POINTER(reg_id));
    g_hash_table_remove(obj->registration_ids, iface);
  }

  // If all interfaces are gone, remove the entire object
  if (g_hash_table_size(obj->registration_ids) == 0) {
    log_info("All interfaces removed for %s, removing object from cache",
             removed_object_path);
    g_hash_table_remove(proxy_state->proxied_objects, removed_object_path);
  } else {
    log_verbose("Object %s still has %u interface(s) remaining",
                removed_object_path, g_hash_table_size(obj->registration_ids));
  }
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  // Free the interface array
  g_strfreev(removed_interfaces);
}

static void on_service_vanished(GDBusConnection *connection G_GNUC_UNUSED,
                                const gchar *name G_GNUC_UNUSED,
                                gpointer user_data G_GNUC_UNUSED) {
  log_info("%s vanished. Exiting", proxy_state->config.source_bus_name);
  g_main_loop_quit(proxy_state->main_loop);
}

// Initialize proxy state
static gboolean init_proxy_state(const ProxyConfig *config) {
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

static gboolean register_nm_secret_agent() {
  const char *nm_secret_agent_xml = g_getenv("NM_SECRET_AGENT_XML");
  const char *object_path = "/org/freedesktop/NetworkManager/SecretAgent";

  if (!nm_secret_agent_xml) {
    log_error("NM secret agent mode enabled but NM_SECRET_AGENT_XML not set");
    return FALSE;
  }

  // Check if already registered
  if (proxy_state->secret_agent_reg_id != 0) {
    log_error("Secret agent already registered (ID: %u), skipping",
              proxy_state->secret_agent_reg_id);
    return TRUE;
  }

  GError *error = nullptr;
  gchar *interface_xml = nullptr;
  GDBusNodeInfo *info = nullptr;

  // Load XML file
  if (!g_file_get_contents(nm_secret_agent_xml, &interface_xml, nullptr,
                           &error)) {
    log_error("Failed to read NM secret agent XML from %s: %s",
              nm_secret_agent_xml, error ? error->message : "unknown error");
    g_clear_error(&error);
    return FALSE;
  }

  // Parse XML
  info = g_dbus_node_info_new_for_xml(interface_xml, &error);
  g_free(interface_xml);

  if (!info) {
    log_error("Failed to parse NM secret agent XML: %s",
              error ? error->message : "unknown error");
    g_clear_error(&error);
    return FALSE;
  }

  // Validate interfaces
  if (!info->interfaces || !info->interfaces[0]) {
    log_error("No interfaces found in NM secret agent XML");
    g_dbus_node_info_unref(info);
    return FALSE;
  }

  // Validate interface name
  const char *expected_interface = DBUS_INTERFACE_SECRET_AGENT;
  if (g_strcmp0(info->interfaces[0]->name, expected_interface) != 0) {
    log_verbose("Unexpected interface: %s (expected %s)",
                info->interfaces[0]->name, expected_interface);
  }

  log_info("Registering secret agent interface %s at %s",
           info->interfaces[0]->name, object_path);

  // Hash table takes ownership
  g_hash_table_insert(proxy_state->node_info_cache, g_strdup("secret_agent"),
                      info);

  // Setup vtable
  static const GDBusInterfaceVTable vtable = {.method_call =
                                                  handle_method_call_generic,
                                              .get_property = nullptr,
                                              .set_property = nullptr,
                                              .padding = {nullptr}};

  // Register object
  proxy_state->secret_agent_reg_id = g_dbus_connection_register_object(
      proxy_state->source_bus, object_path,
      info->interfaces[0], // This pointer is kept alive by cached 'info'
      &vtable,
      g_strdup(object_path), // user_data - will be freed by g_free
      g_free,                // user_data_free_func
      &error);

  if (proxy_state->secret_agent_reg_id == 0) {
    log_error("Failed to register secret agent at %s: %s", object_path,
              error ? error->message : "unknown error");
    g_clear_error(&error);

    // Clean up cache entry since registration failed
    g_hash_table_remove(proxy_state->node_info_cache, "secret_agent");
    return FALSE;
  }

  log_info("Secret agent registered: %s at %s (ID: %u)",
           info->interfaces[0]->name, object_path,
           proxy_state->secret_agent_reg_id);

  return TRUE;
}

static void unregister_nm_secret_agent() {
  if (proxy_state->secret_agent_reg_id == 0) {
    return; // Not registered
  }

  log_info("Unregistering secret agent (ID: %u)",
           proxy_state->secret_agent_reg_id);

  g_rw_lock_writer_lock(&proxy_state->rw_lock);
  // Unregister from D-Bus
  g_dbus_connection_unregister_object(proxy_state->source_bus,
                                      proxy_state->secret_agent_reg_id);

  // Remove from cache (will automatically unref the node_info)
  g_hash_table_remove(proxy_state->node_info_cache, "secret_agent");

  proxy_state->secret_agent_reg_id = 0;
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
}

// Connect to both buses
static gboolean connect_to_buses() {
  GError *error = nullptr;

  // Connect to source bus
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
  // If in NM secret agent mode, register the secret agent interface
  if (proxy_state->config.nm_mode) {
    if (!register_nm_secret_agent()) {
      return FALSE;
    }
  }
  // Connect to target bus
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

// Fetch introspection data from source service
static gboolean fetch_introspection_data() {
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

  const char *xml_data;
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

// Setup signal forwarding with both catch-all and specific PropertiesChanged
// handling
static gboolean setup_signal_forwarding() {
  log_info("Setting up signal forwarding");

  g_rw_lock_writer_lock(&proxy_state->rw_lock);

  // Subscribe to ALL signals from the source bus name
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

// Register interfaces
static gboolean setup_proxy_interfaces() {
  log_info("Setting up proxy interfaces - discovering full object tree");

  // Set up signal forwarding
  if (!setup_signal_forwarding()) {
    return FALSE;
  }

  // First, proxy the D-Bus daemon interface that clients use for service
  // discovery
  if (!discover_and_proxy_object_tree(DBUS_OBJECT_PATH, TRUE)) {
    log_error("Failed to discover and proxy D-Bus daemon interface");
    return FALSE;
  }

  log_info("Object tree proxying complete - %u objects proxied",
           g_hash_table_size(proxy_state->proxied_objects));

  return TRUE;
}

static void on_bus_acquired_for_owner(GDBusConnection *connection,
                                      const gchar *name,
                                      gpointer user_data G_GNUC_UNUSED) {
  log_info("Bus acquired for name: %s", name ? name : "(none)");
  if (!proxy_state)
    return;

  // Keep a reference to the connection
  if (proxy_state->target_bus) {
    g_object_unref(proxy_state->target_bus);
    proxy_state->target_bus = nullptr;
  }
  proxy_state->target_bus = g_object_ref(connection);

  // Register interfaces & set up signal forwarding
  if (!setup_proxy_interfaces()) {
    log_error("Failed to set up interfaces on target bus");
    if (proxy_state->name_owner_watch_id) {
      g_bus_unown_name(proxy_state->name_owner_watch_id);
      proxy_state->name_owner_watch_id = 0;
    }
  }
}

static void on_name_acquired_log(G_GNUC_UNUSED GDBusConnection *conn,
                                 const gchar *name,
                                 gpointer user_data G_GNUC_UNUSED) {
  log_info("Name successfully acquired: %s", name);
}

static void on_name_lost_log(G_GNUC_UNUSED GDBusConnection *conn,
                             const gchar *name,
                             gpointer user_data G_GNUC_UNUSED) {
  log_error("Name lost or failed to acquire: %s", name);
}

// Cleanup function
static void cleanup_proxy_state() {
  if (!proxy_state)
    return;

  // Unregister secret agent
  if (proxy_state->config.nm_mode) {
    unregister_nm_secret_agent();
    if (proxy_state->client_sender_name) {
      g_free(proxy_state->client_sender_name);
      proxy_state->client_sender_name = nullptr;
    }
  }
  // Unregister objects
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
  // Unregister interfaces and clean up
  if (proxy_state->node_info_cache) {
    g_hash_table_destroy(proxy_state->node_info_cache);
  }
  // Unsubscribe from catch-all signal
  if (proxy_state->catch_all_subscription_id && proxy_state->source_bus) {
    g_dbus_connection_signal_unsubscribe(
        proxy_state->source_bus, proxy_state->catch_all_subscription_id);
    proxy_state->catch_all_subscription_id = 0;
  }
  // Unsubscribe from InterfacesAdded signal
  if (proxy_state->catch_interfaces_added_subscription_id &&
      proxy_state->source_bus) {
    g_dbus_connection_signal_unsubscribe(
        proxy_state->source_bus,
        proxy_state->catch_interfaces_added_subscription_id);
    proxy_state->catch_interfaces_added_subscription_id = 0;
  }
  // Unsubscribe from InterfacesRemoved signal
  if (proxy_state->catch_interfaces_removed_subscription_id &&
      proxy_state->source_bus) {
    g_dbus_connection_signal_unsubscribe(
        proxy_state->source_bus,
        proxy_state->catch_interfaces_removed_subscription_id);
    proxy_state->catch_interfaces_removed_subscription_id = 0;
  }
  // Clean up individual signal subscriptions (like PropertiesChanged)
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

  if (proxy_state->sigint_source_id) {
    g_source_remove(proxy_state->sigint_source_id);
    proxy_state->sigint_source_id = 0;
  }
  if (proxy_state->sigterm_source_id) {
    g_source_remove(proxy_state->sigterm_source_id);
    proxy_state->sigterm_source_id = 0;
  }
  g_rw_lock_clear(&proxy_state->rw_lock);
  g_free(proxy_state);
  proxy_state = nullptr;
}

// Signal handler for graceful shutdown
static gboolean signal_handler(void *user_data) {
  int signum = GPOINTER_TO_INT(user_data);
  log_info("Received signal %d, shutting down...", signum);

  // Quit the main loop safely
  if (proxy_state->main_loop) {
    g_main_loop_quit(proxy_state->main_loop);
  }
  return G_SOURCE_REMOVE;
}

// Parse bus type from string
static GBusType parse_bus_type(const char *bus_str) {
  if (g_strcmp0(bus_str, "system") == 0) {
    return G_BUS_TYPE_SYSTEM;
  } else if (g_strcmp0(bus_str, "session") == 0) {
    return G_BUS_TYPE_SESSION;
  }
  return G_BUS_TYPE_SYSTEM; // Default
}

// Validate required proxy configuration parameters
static void validateProxyConfigOrExit(const ProxyConfig *config) {
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

int main(int argc, char *argv[]) {
  // Default configuration
  ProxyConfig config = {.source_bus_name = nullptr,
                        .source_object_path = nullptr,
                        .proxy_bus_name = nullptr,
                        .source_bus_type = G_BUS_TYPE_SYSTEM,
                        .target_bus_type = G_BUS_TYPE_SESSION,
                        .nm_mode = FALSE,
                        .verbose = FALSE,
                        .info = FALSE};

  /* temporary storage for --source-bus-type / --target-bus-type */
  gchar *opt_source_bus_type = nullptr;
  gchar *opt_target_bus_type = nullptr;
  gboolean fatal_warnings = FALSE;

  GOptionEntry entries[] = {
      {"source-bus-name", 0, 0, G_OPTION_ARG_STRING, &config.source_bus_name,
       "D-Bus name of the source", "NAME"},
      {"source-object-path", 0, 0, G_OPTION_ARG_STRING,
       &config.source_object_path, "Object path of the source", "PATH"},
      {"proxy-bus-name", 0, 0, G_OPTION_ARG_STRING, &config.proxy_bus_name,
       "D-Bus name for the proxy", "NAME"},
      {"source-bus-type", 0, 0, G_OPTION_ARG_STRING, &opt_source_bus_type,
       "Bus type of the source (system|session)", "TYPE"},
      {"target-bus-type", 0, 0, G_OPTION_ARG_STRING, &opt_target_bus_type,
       "Bus type of the proxy (system|session)", "TYPE"},
      {"nm-mode", 0, 0, G_OPTION_ARG_NONE, &config.nm_mode,
       "Enable NetworkManager mode", nullptr},
      {"verbose", 0, 0, G_OPTION_ARG_NONE, &config.verbose,
       "Enable verbose output", nullptr},
      {"info", 0, 0, G_OPTION_ARG_NONE, &config.info, "Show additional info",
       nullptr},
      {"fatal-warnings", 0, 0, G_OPTION_ARG_NONE, &fatal_warnings,
       "Crash on warnings (for debugging)", nullptr},
      {nullptr, 0, 0, G_OPTION_ARG_NONE, nullptr, nullptr, nullptr}};

  // Parse command-line options
  GError *error = nullptr;
  GOptionContext *context = g_option_context_new("- D-Bus Proxy");
  g_option_context_add_main_entries(context, entries, nullptr);
  if (!g_option_context_parse(context, &argc, &argv, &error)) {
    log_error("Failed to parse options: %s", error->message);
    g_clear_error(&error);
    g_option_context_free(context);
    return 1;
  }
  g_option_context_free(context);

  if (fatal_warnings) {
    g_setenv("DBUS_FATAL_WARNINGS", "1", TRUE);
  }
  if (opt_source_bus_type) {
    config.source_bus_type = parse_bus_type(opt_source_bus_type);
    g_free(opt_source_bus_type);
  }
  if (opt_target_bus_type) {
    config.target_bus_type = parse_bus_type(opt_target_bus_type);
    g_free(opt_target_bus_type);
  }

  if (config.nm_mode) {
    // In NetworkManager mode, we proxy NetworkManager's D-Bus interface
    if (!config.source_bus_name)
      config.source_bus_name = g_strdup(DBUS_INTERFACE_NETWORK_MANAGER);
    if (!config.source_object_path)
      config.source_object_path = g_strdup(DBUS_OBJECT_PATH_NETWORK_MANAGER);
    if (!config.proxy_bus_name)
      config.proxy_bus_name = g_strdup(DBUS_INTERFACE_NETWORK_MANAGER);

  } else {
    // Ensure required parameters are provided
    if (!config.source_bus_name || config.source_bus_name[0] == '\0' ||
        !config.source_object_path || config.source_object_path[0] == '\0' ||
        !config.proxy_bus_name || config.proxy_bus_name[0] == '\0') {
      log_error("Error: --source-bus-name, --source-object-path, and "
                "--proxy-bus-name are required unless --nm-mode is used.");
      return 1;
    }
  }
  // Validate configuration
  validateProxyConfigOrExit(&config);

  log_info("Starting cross-bus D-Bus proxy");
  log_info("Source: %s%s on %s bus", config.source_bus_name,
           config.source_object_path,
           config.source_bus_type == G_BUS_TYPE_SYSTEM ? "system" : "session");
  log_info("Target: %s on %s bus", config.proxy_bus_name,
           config.target_bus_type == G_BUS_TYPE_SYSTEM ? "system" : "session");

  // Initialize proxy state
  if (!init_proxy_state(&config)) {
    log_error("Failed to initialize proxy state");
    return 1;
  }

  // Connect to both buses
  if (!connect_to_buses()) {
    cleanup_proxy_state();
    return 1;
  }

  // Fetch introspection data from source
  if (!fetch_introspection_data()) {
    cleanup_proxy_state();
    return 1;
  }

  // Start owning the proxy name on the target bus
  proxy_state->name_owner_watch_id = g_bus_own_name(
      proxy_state->config.target_bus_type, proxy_state->config.proxy_bus_name,
      G_BUS_NAME_OWNER_FLAGS_NONE, on_bus_acquired_for_owner,
      on_name_acquired_log, on_name_lost_log, nullptr, nullptr);

  if (proxy_state->name_owner_watch_id == 0) {
    log_error("Failed to own name %s on target bus",
              proxy_state->config.proxy_bus_name);
    cleanup_proxy_state();
    return 1;
  }

  // Watch for the source service to vanish
  proxy_state->source_service_watch_id = g_bus_watch_name(
      proxy_state->config.source_bus_type, proxy_state->config.source_bus_name,
      G_BUS_NAME_WATCHER_FLAGS_NONE,
      nullptr,             // on_name_appeared,
      on_service_vanished, // on_name_vanished
      nullptr,             // user_data
      nullptr              // flags
  );

  if (proxy_state->source_service_watch_id == 0) {
    log_error("Failed to watch name %s on source bus",
              proxy_state->config.source_bus_name);
    if (proxy_state->name_owner_watch_id) {
      g_bus_unown_name(proxy_state->name_owner_watch_id);
      proxy_state->name_owner_watch_id = 0;
    }
    cleanup_proxy_state();
    return 1;
  }

  // Run main loop
  proxy_state->main_loop = g_main_loop_new(nullptr, FALSE);
  g_main_loop_run(proxy_state->main_loop);

  g_main_loop_unref(proxy_state->main_loop);
  cleanup_proxy_state();

  return 0;
}
