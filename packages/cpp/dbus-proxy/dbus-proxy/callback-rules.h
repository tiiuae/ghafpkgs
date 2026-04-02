/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "./dbus_proxy.h"
#include <gio/gio.h>
#include <glib.h>
#ifndef CALLBACK_RULES_H
#define CALLBACK_RULES_H

typedef struct {
  const gchar *name;          // method name
  const gchar *in_signature;  // e.g. "os", "a{sv}", "" (or nullptr)
  const gchar *out_signature; // e.g. "", "b", "o"
} GDBusMethodTable;

struct AgentRule {
  const gchar *bus_name;     // "org.bluez"
  const gchar *manager_path; // "/org/freedesktop/NetworkManager/AgentManager"
  const gchar *manager_interface; // "org.bluez.AgentManager1"
  const gchar *register_method;   // "RegisterAgent"
  const gchar *unregister_method; // "UnregisterAgent"
  // set to true when client sends its agent object path
  // which can be customized
  const gboolean object_path_customisable;

  const gchar *client_object_path; // "/org/bluez/agent"
  const gchar *client_interface;   // "org.bluez.Agent1"
  const GDBusMethodTable
      *client_methods; // NULL-terminated array of method tables
};

extern const AgentRule callbacks_rules[];

const AgentRule *get_callback_rule(const gchar *bus_name,
                                   const gchar *interface_name,
                                   const gchar *method_name);
#endif // CALLBACK_RULES_H
