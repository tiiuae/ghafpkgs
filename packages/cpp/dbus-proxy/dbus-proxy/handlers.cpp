/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "dbus_proxy.h"
#include <gio/gio.h>
#include <glib.h>
#include <string.h>

const gchar *standard_interfaces[] = {DBUS_INTERFACE_INTROSPECTABLE,
                                      DBUS_INTERFACE_PEER,
                                      DBUS_INTERFACE_PROPERTIES, nullptr};

static void proxy_return_error(GDBusMethodInvocation *invocation,
                               GError *error) {
  const gchar *remote = g_dbus_error_get_remote_error(error);

  if (remote) {
    g_dbus_method_invocation_return_dbus_error(invocation, remote,
                                               error->message);
  } else {
    g_dbus_method_invocation_return_gerror(invocation, error);
  }
}

void method_call_reply_callback(GObject *source, GAsyncResult *res,
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
    proxy_return_error(context->invocation, error);
    g_clear_error(&error);
  }

  g_object_unref(context->invocation);
  g_free(context->forward_bus_name);
  g_free(context);
}

// cppcheck-suppress constParameterPointer
void handle_method_call_generic(GDBusConnection *connection,
                                const gchar *sender, const gchar *object_path,
                                const gchar *interface_name,
                                const gchar *method_name, GVariant *parameters,
                                GDBusMethodInvocation *invocation,
                                gpointer user_data) {

  const gchar *target_object_path = static_cast<const gchar *>(user_data);

  log_verbose("Method call: %s.%s on %s from %s (forwarding to %s)",
              interface_name, method_name, object_path, sender,
              target_object_path);

  GDBusConnection *forward_bus = nullptr;
  gchar *forward_bus_name = nullptr;

  if (connection == proxy_state->target_bus) { // call comes from a client
    forward_bus = proxy_state->source_bus;
    forward_bus_name = g_strdup(proxy_state->config.source_bus_name);

    // Handle Register/Unregister methods
    if (g_str_has_prefix(method_name, "Register")) {
      // Add client's agent callback to our registry
      // Skip forwarding the call if the agent is already registered
      if (handle_agent_register_call(sender, object_path, interface_name,
                                     method_name, parameters)) {
        // Generate DBus reply indicating success, predending we handled the
        // registration
        g_dbus_method_invocation_return_value(invocation, nullptr);
        return;
      };
    } else if (g_str_has_prefix(method_name, "Unregister")) {
      // Check if method name starts with Unregister
      log_error("Method %s detected as unregistration method", method_name);
      // Remove client's agent callback from our registry
      // Skip forwarding the call if the agent is already unregistered
      if (handle_agent_unregister_call(sender, object_path, interface_name,
                                       method_name, parameters)) {
        // Generate DBus reply indicating success, predending we handled the
        // unregistration
        g_dbus_method_invocation_return_value(invocation, nullptr);
        return;
      }
    }
  } else { // call comes from source bus, forward back to client

    forward_bus = proxy_state->target_bus;
    // Get from registry the sender name for this connection to be able to
    // forward the call back to the right client. If no sender name is found, it
    // means this call was not expected, so we should return an error to avoid
    // forwarding it to a random client+
    forward_bus_name =
        g_strdup(get_agent_name(object_path, interface_name, method_name));
    if (!forward_bus_name) {
      log_error(
          "No sender name found for connection, cannot forward method call");
      g_dbus_method_invocation_return_error(
          invocation, G_DBUS_ERROR, G_DBUS_ERROR_FAILED,
          "Internal proxy error: agent callback registration not found for "
          "this method call");
      return;
    }

    log_verbose("Forwarding agent call to client: %s", forward_bus_name);
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

void on_signal_received_catchall(GDBusConnection *connection G_GNUC_UNUSED,
                                 const gchar *sender_name,
                                 const gchar *object_path,
                                 const gchar *interface_name,
                                 const gchar *signal_name, GVariant *parameters,
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

void update_object_with_new_interfaces(const gchar *object_path,
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
    discover_and_proxy_object_tree(object_path, NULL, TRUE);
    return;
  }

  // Iterate through the new interfaces
  GVariantIter iter;
  const gchar *interface_name;
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

void on_interfaces_added(GDBusConnection *connection G_GNUC_UNUSED,
                         const gchar *sender_name G_GNUC_UNUSED,
                         const gchar *object_path, const gchar *interface_name,
                         const gchar *signal_name, GVariant *parameters,
                         gpointer user_data G_GNUC_UNUSED) {
  const gchar *added_object_path;
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

void on_interfaces_removed(GDBusConnection *connection G_GNUC_UNUSED,
                           const gchar *sender_name G_GNUC_UNUSED,
                           const gchar *object_path G_GNUC_UNUSED,
                           const gchar *interface_name G_GNUC_UNUSED,
                           const gchar *signal_name G_GNUC_UNUSED,
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
    gchar *iface_list = g_strjoinv(", ", removed_interfaces);
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
    const gchar *iface = removed_interfaces[i];

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
    gchar *cache_key = g_strdup_printf("%s:%s", removed_object_path, iface);
    g_hash_table_remove(proxy_state->node_info_cache, cache_key);
    g_free(cache_key);

    if (success) {
      log_verbose("Unregistered interface %s on %s (reg_id %u)", iface,
                  removed_object_path, reg_id);
    } else {
      log_error("Failed to unregister interface %s on %s (reg_id %u)", iface,
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

void on_service_vanished(GDBusConnection *connection G_GNUC_UNUSED,
                         const gchar *name G_GNUC_UNUSED,
                         gpointer user_data G_GNUC_UNUSED) {
  log_info("%s vanished. Exiting", proxy_state->config.source_bus_name);
  g_main_loop_quit(proxy_state->main_loop);
}

gboolean signal_handler(void *user_data) {
  int signum = GPOINTER_TO_INT(user_data);
  log_info("Received signal %d, shutting down...", signum);

  // Quit the main loop safely
  if (proxy_state->main_loop) {
    g_main_loop_quit(proxy_state->main_loop);
  }
  return G_SOURCE_REMOVE;
}
