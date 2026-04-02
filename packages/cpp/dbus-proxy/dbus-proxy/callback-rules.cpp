/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "dbus_proxy.h"

const GDBusMethodTable nm_agent_methods[] = {
    {"GetSecrets", "a{sa{sv}}osasu", "a{sa{sv}}"},
    {"CancelGetSecrets", "os", ""},
    {"SaveSecrets", "a{sa{sv}}o", ""},
    {"DeleteSecrets", "a{sa{sv}}o", ""},
    {nullptr, nullptr, nullptr}};

const GDBusMethodTable bluez_agent_methods[] = {
    {"RequestPinCode", "o", "s"},      {"DisplayPinCode", "os", ""},
    {"RequestPasskey", "o", "u"},      {"DisplayPasskey", "ouq", ""},
    {"RequestConfirmation", "ou", ""}, {"RequestAuthorization", "o", ""},
    {"AuthorizeService", "os", ""},    {"Cancel", "", ""},
    {nullptr, nullptr, nullptr}};

const GDBusMethodTable obex_agent_methods[] = {{"CreateSession", "s", "sa{sv}"},
                                               {"RemoveSession", "o", ""},
                                               {nullptr, nullptr, nullptr}};

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

    {.bus_name = DBUS_BLUEZ_NAME,
     .manager_path = "/org/bluez",
     .manager_interface = "org.bluez.AgentManager1",
     .register_method = "RegisterAgent",
     .unregister_method = "UnregisterAgent",
     .object_path_customisable = FALSE,

     .client_object_path = DBUS_BLUEZ_AGENT_PATH,
     .client_interface = DBUS_BLUEZ_AGENT_INTERFACE,
     .client_methods = bluez_agent_methods},

    {.bus_name = DBUS_OBEX_NAME,
     .manager_path = "/org/bluez",
     .manager_interface = "org.bluez.obex.AgentManager1",
     .register_method = "RegisterAgent",
     .unregister_method = "UnregisterAgent",
     .object_path_customisable = FALSE,

     .client_object_path = DBUS_OBEX_AGENT_PATH,
     .client_interface = DBUS_OBEX_AGENT_INTERFACE,
     .client_methods = obex_agent_methods},

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
