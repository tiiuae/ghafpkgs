/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "dbus_proxy.h"

const gchar *nm_agent_methods[] = {"GetSecrets", "CancelGetSecrets",
                                   "SaveSecrets", "DeleteSecrets", nullptr};

const AgentRule callbacks_rules[] = {
    {.bus_name = DBUS_NETWORK_MANAGER_NAME,
     .manager_path = "/org/freedesktop/NetworkManager/AgentManager",
     .manager_interface = "org.freedesktop.NetworkManager.AgentManager",
     .register_method = "Register",
     .unregister_method = "Unregister",
     .object_path_customisable = FALSE,
     .client_object_path = DBUS_NM_AGENT_PATH,
     .client_interface = DBUS_INTERFACE_SECRET_AGENT,
     .client_methods = nm_agent_methods},

    {.bus_name = DBUS_NETWORK_MANAGER_NAME,
     .manager_path = "/org/freedesktop/NetworkManager/AgentManager",
     .manager_interface = "org.freedesktop.NetworkManager.AgentManager",
     .register_method = "RegisterWithCapabilities",
     .unregister_method = "Unregister",
     .object_path_customisable = FALSE,

     .client_object_path = DBUS_NM_AGENT_PATH,
     .client_interface = DBUS_INTERFACE_SECRET_AGENT,
     .client_methods = nm_agent_methods},
    {nullptr, nullptr, nullptr, nullptr, nullptr, FALSE, nullptr, nullptr,
     nullptr}};

const AgentRule *get_callback_rule(const gchar *bus_name,
                                   const gchar *interface_name,
                                   const gchar *method_name) {
  for (int i = 0; callbacks_rules[i].bus_name != nullptr; i++) {
    const AgentRule *rule = &callbacks_rules[i];
    if (g_strcmp0(rule->bus_name, bus_name) == 0 &&
        g_strcmp0(rule->manager_interface, interface_name) == 0 &&
        (g_strcmp0(rule->register_method, method_name) == 0 ||
         g_strcmp0(rule->unregister_method, method_name) == 0)) {
      return rule;
    }
  }
  return nullptr;
}
