/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

/* Shared header for dbus-proxy split files */
#ifndef GHAF_DBUS_PROXY_H
#define GHAF_DBUS_PROXY_H

#include "./callback-rules.h"
#include "./gdbusprivate.h"
#include <gio/gio.h>
#include <glib.h>

// Configuration structure
typedef struct {
  gchar *source_bus_name;
  gchar *source_object_path;
  gchar *proxy_bus_name;
  GBusType source_bus_type;
  GBusType target_bus_type;
  gboolean verbose;
  gboolean info;
} ProxyConfig;

// Forward declarations
typedef struct ProxiedObject ProxiedObject;
typedef struct {
  GDBusConnection *source_bus;
  GDBusConnection *target_bus;
  GDBusNodeInfo *introspection_data;
  GHashTable *registered_objects;
  GHashTable *signal_subscriptions;
  ProxyConfig config;
  guint dbus_agent_reg_id;
  gchar *client_sender_name;
  guint name_owner_watch_id;
  guint source_service_watch_id;
  guint catch_all_subscription_id;
  guint catch_interfaces_added_subscription_id;
  guint catch_interfaces_removed_subscription_id;
  GHashTable *proxied_objects;
  GHashTable *node_info_cache;
  GPtrArray *agents_registry; /* jarekk: temporary, to be replaced */
  GHashTable *senders_registry;
  GRWLock rw_lock;
  guint sigint_source_id;
  guint sigterm_source_id;
  GMainLoop *main_loop;
} ProxyState;

struct ProxiedObject {
  gchar *object_path;
  GDBusNodeInfo *node_info;
  GHashTable *registration_ids;
};

struct AgentData {
  const gchar *owner;
  const gchar *object_path;
  const gchar *unique_object_path;
  const struct AgentRule *rule;
  gint agent_object_reg_id;
  gint name_change_reg_id;
  GDBusInterfaceInfo *iface;
};

struct MethodCallContext {
  GDBusMethodInvocation *invocation;
  gchar *forward_bus_name;
};

extern ProxyState *proxy_state;

/* Logging */
void log_verbose(const gchar *format, ...) G_GNUC_PRINTF(1, 2);
void log_error(const gchar *format, ...) G_GNUC_PRINTF(1, 2);
void log_info(const gchar *format, ...) G_GNUC_PRINTF(1, 2);

/* Initialization / cleanup */
gboolean init_proxy_state(const ProxyConfig *config);
void cleanup_proxy_state(void);

/* Bus / introspection */
gboolean connect_to_buses(void);
gboolean fetch_introspection_data(void);

/* Discovery / proxying */
gboolean discover_and_proxy_object_tree(const gchar *base_path,
                                        gchar **foundObjectManagerPath,
                                        gboolean need_lock);
gboolean proxy_single_object(const gchar *object_path, GDBusNodeInfo *node_info,
                             gboolean need_lock);
gboolean register_single_interface(const gchar *object_path,
                                   const gchar *interface_name,
                                   ProxiedObject *proxied_obj);
void update_object_with_new_interfaces(const gchar *object_path,
                                       GVariant *interfaces_dict);

/* Signal handlers and method/property handlers */
gboolean signal_handler(void *user_data);
void on_signal_received_catchall(GDBusConnection *connection,
                                 const gchar *sender_name,
                                 const gchar *object_path,
                                 const gchar *interface_name,
                                 const gchar *signal_name, GVariant *parameters,
                                 gpointer user_data);
void on_interfaces_added(GDBusConnection *connection, const gchar *sender_name,
                         const gchar *object_path, const gchar *interface_name,
                         const gchar *signal_name, GVariant *parameters,
                         gpointer user_data);
void on_interfaces_removed(GDBusConnection *connection,
                           const gchar *sender_name, const gchar *object_path,
                           const gchar *interface_name,
                           const gchar *signal_name, GVariant *parameters,
                           gpointer user_data);

/* Other callbacks used across files */
void on_bus_acquired_for_owner(GDBusConnection *connection, const gchar *name,
                               gpointer user_data G_GNUC_UNUSED);
void on_name_acquired_log(GDBusConnection *conn, const gchar *name,
                          gpointer user_data G_GNUC_UNUSED);
void on_name_lost_log(GDBusConnection *conn, const gchar *name,
                      gpointer user_data G_GNUC_UNUSED);
void on_service_vanished(GDBusConnection *connection, const gchar *name,
                         gpointer user_data G_GNUC_UNUSED);

/* Standard D-Bus interfaces list (defined in one .cpp) */
extern const gchar *standard_interfaces[];

/* Method/property handlers used by vtables */
void handle_method_call_generic(GDBusConnection *connection,
                                const gchar *sender, const gchar *object_path,
                                const gchar *interface_name,
                                const gchar *method_name, GVariant *parameters,
                                GDBusMethodInvocation *invocation,
                                gpointer user_data);

/* Utilities */
GBusType parse_bus_type(const gchar *bus_str);
void validateProxyConfigOrExit(const ProxyConfig *config);
void method_call_reply_callback(GObject *source, GAsyncResult *res,
                                gpointer user_data);
GDBusInterfaceInfo *build_interface_info(const gchar *iface_name,
                                         const gchar **methods);
void free_interface_info(GDBusInterfaceInfo *iface);
const gchar *get_agent_name(const gchar *object_path,
                            const gchar *interface_name,
                            const gchar *method_name);

gboolean register_agent_callback(const gchar *sender, const gchar *object_path,
                                 const gchar *unique_object_path,
                                 const gchar *interface_name,
                                 const gchar *method_name, const guint reg_id,
                                 GDBusInterfaceInfo *ifac);
gboolean handle_agent_register_call(const gchar *sender,
                                    const gchar *object_path,
                                    const gchar *interface_name,
                                    const gchar *method_name,
                                    GVariant *parameters);
gboolean handle_agent_unregister_call(const gchar *sender,
                                      const gchar *object_path,
                                      const gchar *interface_name,
                                      const gchar *method_name,
                                      GVariant *parameters);
void unregister_all_agent_registrations();
void free_agent_callback_data(gpointer ptr);
GDBusConnection *get_sender_dbus_connection(const gchar *sender_name);
// jarekk gchar *get_sender_name_from_connection(GDBusConnection *connection);
GHashTable *get_sender_callbacks(const gchar *sender_name);

#endif // GHAF_DBUS_PROXY_H
