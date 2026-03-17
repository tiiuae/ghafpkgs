/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "dbus_proxy.h"
#include <gio/gio.h>
#include <glib-unix.h>
#include <glib.h>
#include <string.h>

static void on_name_owner_changed(GDBusConnection *connection G_GNUC_UNUSED,
                                  const gchar *sender_name G_GNUC_UNUSED,
                                  const gchar *object_path G_GNUC_UNUSED,
                                  const gchar *interface_name G_GNUC_UNUSED,
                                  const gchar *signal_name G_GNUC_UNUSED,
                                  GVariant *parameters,
                                  gpointer user_data G_GNUC_UNUSED);

void free_agent_callback_data(gpointer ptr) {
  struct AgentData *data = static_cast<struct AgentData *>(ptr);

  if (!data)
    return;

  // Unsubscribe from NameOwnerChanged signal
  g_dbus_connection_signal_unsubscribe(proxy_state->target_bus,
                                       data->name_change_reg_id);
  log_verbose("Freeing agent data: Unregistered subscription for "
              "'NameOwnerChanged' signal "
              "owner %s reg_id %d path %s",
              data->owner, data->name_change_reg_id, data->object_path);
  // if agent_object_reg_id = 0 it means another registration is done
  // for the same object path, so we should not notify server with Unregister it
  // yet
  if (data->agent_object_reg_id) {
    if (!g_dbus_connection_unregister_object(proxy_state->source_bus,
                                             data->agent_object_reg_id)) {
      log_error("Freeing agent data: Failed to unregister object owner %s "
                "unique path %s path %s",
                data->owner, data->unique_object_path, data->object_path);
    } else {
      log_verbose("Freeing agent data: Unregistered agent object for owner %s "
                  "unique path %s path %s",
                  data->owner, data->unique_object_path, data->object_path);
    }
    /* iface is allocated only if agent's object is registered */
    free_interface_info(data->iface);
  }

  g_free(const_cast<gchar *>(data->owner));
  g_free(const_cast<gchar *>(data->object_path));
  g_free(const_cast<gchar *>(data->unique_object_path));
  g_free(data);
}

/* Called when a DBus client registers a callback */
gboolean register_agent_callback(const gchar *sender, const gchar *object_path,
                                 const gchar *unique_object_path,
                                 const gchar *interface_name,
                                 const gchar *method_name,
                                 const guint agent_object_reg_id,
                                 GDBusInterfaceInfo *iface) {

  /* Find callback rule by interface name */
  const AgentRule *rule = get_callback_rule(proxy_state->config.source_bus_name,
                                            interface_name, method_name);
  if (!rule) {
    log_error("No callback rule found for %s %s.%s", sender, interface_name,
              method_name);
    return FALSE;
  }

  struct AgentData *data = g_new0(struct AgentData, 1);
  data->owner = g_strdup(sender);
  data->object_path = g_strdup(object_path);
  data->unique_object_path = g_strdup(unique_object_path);
  data->agent_object_reg_id = agent_object_reg_id;
  data->rule = rule;
  data->iface = iface;
  data->name_change_reg_id = g_dbus_connection_signal_subscribe(
      proxy_state->target_bus, nullptr, "org.freedesktop.DBus",
      "NameOwnerChanged", "/org/freedesktop/DBus", nullptr,
      G_DBUS_SIGNAL_FLAGS_NONE, on_name_owner_changed, (gpointer)data->owner,
      nullptr);

  if (data->name_change_reg_id == 0) {
    log_error("Failed to subscribe to NameOwnerChanged signal for sender %s",
              sender);
    g_free((gpointer)data->owner);
    g_free((gpointer)data->object_path);
    g_free((gpointer)data->unique_object_path);
    free_interface_info(data->iface);
    g_free(data);
    return FALSE;
  }

  g_rw_lock_writer_lock(&proxy_state->rw_lock);
  g_ptr_array_insert(proxy_state->agents_registry, -1, data);
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);

  log_info("Registered callback rule for %s %s reg id %d", sender,
           unique_object_path, agent_object_reg_id);

  return TRUE;
}

gboolean unique_path_equal(gconstpointer a, gconstpointer b) {
  const AgentData *data = static_cast<const AgentData *>(a);
  const gchar *unique_object_path = static_cast<const gchar *>(b);
  return g_strcmp0(data->unique_object_path, unique_object_path) == 0;
}

AgentData *find_registered_path(const gchar *unique_path) {
  guint index;
  AgentData *result = NULL;

  g_rw_lock_reader_lock(&proxy_state->rw_lock);

  if (!proxy_state->agents_registry || !unique_path) {
    goto out;
  }
  if (!g_ptr_array_find_with_equal_func(proxy_state->agents_registry,
                                        (gconstpointer)unique_path,
                                        unique_path_equal, &index)) {
    goto out;
  }

  result = static_cast<AgentData *>(
      g_ptr_array_index(proxy_state->agents_registry, index));

out:
  g_rw_lock_reader_unlock(&proxy_state->rw_lock);
  return result;
}

static const GDBusMethodTable *
find_method_by_name_ptrs(const GDBusMethodTable *methods, const gchar *name) {
  for (guint i = 0; methods[i].name != nullptr; ++i) {
    if (g_strcmp0(methods[i].name, name) == 0) {
      return &methods[i];
    }
  }
  return nullptr;
}

const gchar *get_agent_name(const gchar *object_path,
                            const gchar *interface_name,
                            const gchar *method_name) {
  AgentData *result = find_registered_path(object_path);
  if (result &&
      g_strcmp0(result->rule->client_interface, interface_name) == 0 &&
      !find_method_by_name_ptrs(result->rule->client_methods, method_name)) {
    log_error("No agent found for object path %s call %s.%s", object_path,
              interface_name, method_name);
    return nullptr;
  }
  if (result) {
    log_verbose("Found agent for path %s: owner %s unique path %s", object_path,
                result->owner, result->unique_object_path);
  } else {
    log_error("No agent found for object path %s call %s.%s", object_path,
              interface_name, method_name);
  }
  return result ? result->owner : nullptr;
}

void unregister_all_agent_registrations() {
  g_rw_lock_writer_lock(&proxy_state->rw_lock);
  g_ptr_array_free(proxy_state->agents_registry, TRUE);
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
}

static GDBusArgInfo **build_arg_info_list(const gchar *sig,
                                          const gchar *prefix) {
  if (!sig || sig[0] == '\0')
    return nullptr;

  GPtrArray *arr = g_ptr_array_new();
  const gchar *p = sig;
  guint idx = 0;

  while (*p) {
    const gchar *end = nullptr;
    if (!g_variant_type_string_scan(p, nullptr, &end)) {
      // invalid signature: free partial
      for (guint i = 0; i < arr->len; ++i) {
        auto *a = static_cast<GDBusArgInfo *>(g_ptr_array_index(arr, i));
        g_free(a->name);
        g_free(a->signature);
        g_free(a);
      }
      g_ptr_array_free(arr, TRUE);
      return nullptr;
    }

    GDBusArgInfo *arg = g_new0(GDBusArgInfo, 1);
    arg->ref_count = 1;
    arg->name = g_strdup_printf("%s%u", prefix, idx++); // in0, in1, out0...
    arg->signature = g_strndup(p, end - p);
    g_ptr_array_add(arr, arg);

    p = end;
  }

  g_ptr_array_add(arr, nullptr); // NULL-terminate
  return reinterpret_cast<GDBusArgInfo **>(g_ptr_array_free(arr, FALSE));
}

static void free_arg_info_list(GDBusArgInfo **args) {
  if (!args)
    return;
  for (guint i = 0; args[i] != nullptr; ++i) {
    g_free(args[i]->name);
    g_free(args[i]->signature);
    g_free(args[i]);
  }
  g_free(args);
}

void free_interface_info(GDBusInterfaceInfo *iface) {
  if (!iface)
    return;
  for (int i = 0; iface->methods && iface->methods[i]; i++) {
    free_arg_info_list(iface->methods[i]->in_args);
    free_arg_info_list(iface->methods[i]->out_args);
    g_free(iface->methods[i]->name);
    g_free(iface->methods[i]);
  }
  g_free(iface->methods);
  g_free(iface->name);
  g_free(iface);
}

// Input: parameters is "(o)" or "(os)"
// Output: new variant with first field replaced by unique_agent_path.
// Caller owns returned GVariant* (g_variant_unref when done).
static GVariant *
replace_first_with_unique_agent_path(GVariant *parameters,
                                     const gchar *unique_agent_path) {
  if (!parameters || !unique_agent_path) {
    return nullptr;
  }

  if (g_variant_is_of_type(parameters, G_VARIANT_TYPE("(o)"))) {
    return g_variant_new("(o)", unique_agent_path);
  }

  if (g_variant_is_of_type(parameters, G_VARIANT_TYPE("(os)"))) {
    const gchar *old_path = nullptr; // ignored
    const gchar *s = nullptr;
    g_variant_get(parameters, "(&o&s)", &old_path, &s);
    return g_variant_new("(os)", unique_agent_path, s);
  }

  return nullptr; // unsupported type
}

GDBusInterfaceInfo *build_interface_info(const AgentRule *rule) {
  GDBusInterfaceInfo *iface = g_new0(GDBusInterfaceInfo, 1);
  iface->ref_count = 1;
  iface->name = g_strdup(rule->client_interface);

  int method_count = 0;
  for (int i = 0; rule->client_methods[i].name != nullptr; i++) {
    method_count++;
  }
  iface->methods = g_new0(GDBusMethodInfo *, method_count + 1);

  for (int i = 0; i < method_count; i++) {
    GDBusMethodInfo *m = g_new0(GDBusMethodInfo, 1);
    m->ref_count = 1;
    m->name = g_strdup(rule->client_methods[i].name);
    m->in_args =
        build_arg_info_list(rule->client_methods[i].in_signature, "in");
    m->out_args =
        build_arg_info_list(rule->client_methods[i].out_signature, "out");
    iface->methods[i] = m;
  }

  return iface;
}

gboolean handle_agent_register_call(const gchar *sender,
                                    const gchar *object_path G_GNUC_UNUSED,
                                    const gchar *interface_name,
                                    const gchar *method_name,
                                    GVariant *parameters,
                                    GDBusMethodInvocation *invocation) {

  const AgentRule *rule = get_callback_rule(proxy_state->config.source_bus_name,
                                            interface_name, method_name);

  if (!rule) {
    log_error("No callback rule found for %s %s.%s", sender, interface_name,
              method_name);
    return FALSE;
  }

  log_info("Handling register call for %s %s.%s", sender, interface_name,
           method_name);

  // Generate unique object path for this registration, combining sender name
  // and agent path from parameters if needed, to allow multiple registrations
  // from the same sender
  gchar *unique_agent_path = NULL;
  const gchar *agent_path = NULL;

  if (rule->object_path_customisable) {
    // Expect the first parameter to be an object path, extract it and combine
    // with sender to create unique path for this registration. This allows
    // multiple clients to register their agents with different paths, while
    // Extract agent path from parameters
    GVariant *param = g_variant_get_child_value(parameters, 0);
    if (g_variant_is_of_type(param, G_VARIANT_TYPE_OBJECT_PATH)) {
      agent_path = g_variant_get_string(param, nullptr);
    } else {
      log_error("Expected first parameter to be an object path string, but got "
                "different type");
      g_variant_unref(param);
      return FALSE;
    }
    g_variant_unref(param);

    if (!agent_path) {
      log_error("Failed to extract agent path from parameters");
      return FALSE;
    }
    log_info("Extracted agent path from parameters: %s", agent_path);

    unique_agent_path = g_strdup_printf("%s/%s", agent_path, sender);
    g_strdelimit(unique_agent_path, ".:", '_');
  } else {
    unique_agent_path = g_strdup(rule->client_object_path);
  }

  // Register a new object if already not registered
  AgentData *path_registered = find_registered_path(unique_agent_path);
  guint reg_id = 0;
  GDBusInterfaceInfo *iface = nullptr;
  if (path_registered) {
    if (g_strcmp0(path_registered->owner, sender) == 0) {
      log_error("Sender %s is already registered at path %s", sender,
                unique_agent_path);
      g_free(unique_agent_path);
      return TRUE;
    }
    log_info("Sender %s attempts to register agent at path %s (%s.%s)", sender,
             unique_agent_path, interface_name, method_name);
  } else {
    // Register new object
    GError *error = NULL;
    iface = build_interface_info(rule);

    static const GDBusInterfaceVTable vtable = {.method_call =
                                                    handle_method_call_generic,
                                                .get_property = NULL,
                                                .set_property = NULL,
                                                .padding = {nullptr}};

    reg_id = g_dbus_connection_register_object(
        proxy_state->source_bus, unique_agent_path, iface, &vtable,
        g_strdup(agent_path), // user_data
        g_free,               // free user_data
        &error);

    if (reg_id == 0) {
      log_error("Failed to register callback object for %s at %s: %s",
                interface_name, unique_agent_path,
                error ? error->message : "unknown");
      g_clear_error(&error);
      free_interface_info(iface);
      g_free(unique_agent_path);
      return FALSE;
    }
  }

  // Store registration in registry with sender and unique path, so we can
  // properly handle multiple registrations from the same sender and cleanup on
  // unregistration or NameOwnerChanged
  if (register_agent_callback(sender, rule->client_object_path,
                              unique_agent_path, interface_name, method_name,
                              reg_id, iface)) {
    log_info(
        "Callback registered: sender %s path %s unique %s (%s.%s) reg_id %u",
        sender, rule->client_object_path, unique_agent_path, interface_name,
        method_name, reg_id);
  } else {
    log_error("Failed to store callback registration");
    g_dbus_connection_unregister_object(proxy_state->source_bus, reg_id);
    g_free(unique_agent_path);
    return FALSE;
  }

  if (reg_id == 0) { // Inform caller to skip registration call on the server
                     // side, because this was just a registry update for an
                     // already registered path
    g_dbus_method_invocation_return_value(invocation, nullptr);
    g_free(unique_agent_path);
    return TRUE;

  } else {
    GVariant *params =
        replace_first_with_unique_agent_path(parameters, unique_agent_path);
    g_free(unique_agent_path);
    if (!params) {
      log_error("Failed to build parameters for method call");
      g_dbus_method_invocation_return_value(invocation, nullptr);
      return TRUE;
    }

    MethodCallContext *context = g_new0(MethodCallContext, 1);
    context->invocation = invocation;
    context->forward_bus_name = g_strdup(proxy_state->config.source_bus_name);

    g_object_ref(invocation);

    // Forward the Register call to the server with the unique path as a
    // parameter, so the server can identify the caller if needed. The reply
    // will be handled in the generic method call handler, which will route it
    // back to the correct invocation.
    g_dbus_connection_call(proxy_state->source_bus,
                           proxy_state->config.source_bus_name, object_path,
                           interface_name, method_name, params, nullptr,
                           G_DBUS_CALL_FLAGS_NONE, -1, nullptr,
                           method_call_reply_callback, context);
    return TRUE;
  }

  return FALSE;
}

gboolean handle_agent_unregister_call(const gchar *sender,
                                      const gchar *object_path,
                                      const gchar *interface_name,
                                      const gchar *method_name,
                                      GVariant *parameters,
                                      GDBusMethodInvocation *invocation) {
  log_info("Handling callback unregistration for %s at %s method %s", sender,
           object_path, method_name);
  /* Find registration ID for this sender by sender */
  g_rw_lock_writer_lock(&proxy_state->rw_lock);
  // Iterate over the agents registry to find all registrations for this sender
  // and remove them
  gboolean request_handled = FALSE;
  for (guint i = 0; i < proxy_state->agents_registry->len; i++) {
    AgentData *data = static_cast<AgentData *>(
        g_ptr_array_index(proxy_state->agents_registry, i));
    if (g_strcmp0(data->owner, sender) == 0 &&
        g_strcmp0(data->rule->manager_path, object_path) == 0 &&
        g_strcmp0(data->rule->manager_interface, interface_name) == 0 &&
        g_strcmp0(data->rule->unregister_method, method_name) == 0) {
      log_info("Found Unregister data for sender %s unique path %s path %s",
               sender, object_path, method_name);
      if (data->agent_object_reg_id == 0) {
        log_info("This was a secondary registration for sender %s, skipping "
                 "unregistration on the server",
                 sender);
        g_dbus_method_invocation_return_value(invocation, nullptr);
        request_handled = TRUE;
      } else {
        // We need to call Unregister method on the server for this callback to
        // allow proper cleanup on the server side.
        // Rewrite parameters with the unique agent path if needed, so server
        // can identify the caller. This is needed in case of multiple
        // registrations from the same sender, to allow proper unregistration of
        // the correct callback on the server side.
        GVariant *params = nullptr;
        params = replace_first_with_unique_agent_path(parameters,
                                                      data->unique_object_path);
        g_variant_unref(parameters);

        if (!params) {
          log_error("Failed to build parameters for Unregister method call");
          g_dbus_method_invocation_return_value(invocation, nullptr);
          g_rw_lock_writer_unlock(&proxy_state->rw_lock);
          return TRUE;
        }

        MethodCallContext *context = g_new0(MethodCallContext, 1);
        context->invocation = invocation;
        context->forward_bus_name =
            g_strdup(proxy_state->config.source_bus_name);
        g_object_ref(invocation);

        g_dbus_connection_call(proxy_state->source_bus,
                               proxy_state->config.source_bus_name, object_path,
                               interface_name, method_name, params, nullptr,
                               G_DBUS_CALL_FLAGS_NONE, -1, nullptr,
                               method_call_reply_callback, context);
      }
      g_ptr_array_remove(proxy_state->agents_registry, data);
      request_handled = TRUE;
      break;
    }
  }
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
  return request_handled;
}

static void on_name_owner_changed(GDBusConnection *connection G_GNUC_UNUSED,
                                  const gchar *sender_name G_GNUC_UNUSED,
                                  const gchar *object_path G_GNUC_UNUSED,
                                  const gchar *interface_name G_GNUC_UNUSED,
                                  const gchar *signal_name G_GNUC_UNUSED,
                                  GVariant *parameters,
                                  gpointer user_data G_GNUC_UNUSED) {

  const gchar *dbus_name;
  const gchar *old_owner;
  const gchar *new_owner;
  g_variant_get(parameters, "(&s&s&s)", &dbus_name, &old_owner, &new_owner);

  /* Ignore new client notification */
  if (old_owner == nullptr || old_owner[0] == '\0') {
    return;
  }

  /* Ignore client rename */
  if (new_owner && new_owner[0] != '\0') {
    log_error("Sender %s renamed from %s to %s, unsupported scenario, ignoring",
              dbus_name, old_owner, new_owner);
    return;
  }

  // Find and release all callbacks associated with the old owner
  g_rw_lock_writer_lock(&proxy_state->rw_lock);

  // We are dealing usually with very few callbacks per sender, so iterating
  // over the array should be fine
  // We are dealing usually with very few callbacks per sender, so iterating
  for (guint i = 0; i < proxy_state->agents_registry->len; i++) {
    AgentData *data = static_cast<AgentData *>(
        g_ptr_array_index(proxy_state->agents_registry, i));
    if (g_strcmp0(data->owner, old_owner) == 0) {
      log_info(
          "On NameOwnerChanged: unregistering agent registration for sender %s",
          data->owner);
      // Need to call Unregister method on the server for this callback if it
      // was registered by the client, to allow proper cleanup on the server
      // side. If agent_object_reg_id is 0 it means another registration is done
      // for the same object path, so we should not notify server with
      // Unregister it yet
      if (data->agent_object_reg_id) {
        GError *error = NULL;
        GVariant *parameters = nullptr;
        if (data->rule->object_path_customisable) {
          parameters = g_variant_new("(o)", data->unique_object_path);
        }
        g_dbus_connection_call_sync(
            proxy_state->source_bus, proxy_state->config.source_bus_name,
            data->rule->manager_path, data->rule->manager_interface,
            data->rule->unregister_method, parameters, nullptr,
            G_DBUS_CALL_FLAGS_NONE, -1, nullptr, &error);
        if (error) {
          log_error("Failed to call Unregister method for sender %s: %s",
                    data->owner, error->message);
          g_clear_error(&error);
        } else {
          log_verbose("Called Unregister method for sender %s successfully",
                      data->owner);
        }
      } else {
        log_verbose("Skipping Unregister call for sender %s because this was a "
                    "secondary registration",
                    data->owner);
      }
      g_ptr_array_remove(proxy_state->agents_registry, data);
      i--; // Adjust index after removal
    }
  }
  g_rw_lock_writer_unlock(&proxy_state->rw_lock);
}
