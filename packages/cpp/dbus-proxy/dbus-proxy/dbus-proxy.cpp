/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

/*
 * Cross-Bus GDBus Proxy that:
 * 1. Connects to two different D-Bus buses (source and target).
 * 2. Fetches introspection data from source service on source bus.
 * 3. Exposes that interface on target bus with proxy name.
 * 4. Forwards method calls from target bus to source bus.
 * 5. Forwards signals from source bus to target bus.
 * 6. Handles properties synchronization between buses.
 */

#include "dbus_proxy.h"

int main(int argc, gchar *argv[]) {
  // Default configuration
  ProxyConfig config = {.source_bus_name = nullptr,
                        .source_object_path = nullptr,
                        .proxy_bus_name = nullptr,
                        .source_bus_type = G_BUS_TYPE_SYSTEM,
                        .target_bus_type = G_BUS_TYPE_SESSION,
                        .verbose = FALSE,
                        .info = FALSE};

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
      {"verbose", 0, 0, G_OPTION_ARG_NONE, &config.verbose,
       "Enable verbose output", nullptr},
      {"info", 0, 0, G_OPTION_ARG_NONE, &config.info, "Show additional info",
       nullptr},
      {"fatal-warnings", 0, 0, G_OPTION_ARG_NONE, &fatal_warnings,
       "Crash on warnings (for debugging)", nullptr},
      {nullptr, 0, 0, G_OPTION_ARG_NONE, nullptr, nullptr, nullptr}};

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

  if (!config.source_bus_name || config.source_bus_name[0] == '\0' ||
      !config.source_object_path || config.source_object_path[0] == '\0' ||
      !config.proxy_bus_name || config.proxy_bus_name[0] == '\0') {
    log_error("Error: --source-bus-name, --source-object-path, and "
              "--proxy-bus-name are required.");
    return 1;
  }
  validateProxyConfigOrExit(&config);

  log_info("Starting cross-bus D-Bus proxy");
  log_info("Source: %s%s on %s bus", config.source_bus_name,
           config.source_object_path,
           config.source_bus_type == G_BUS_TYPE_SYSTEM ? "system" : "session");
  log_info("Target: %s on %s bus", config.proxy_bus_name,
           config.target_bus_type == G_BUS_TYPE_SYSTEM ? "system" : "session");

  if (!init_proxy_state(&config)) {
    log_error("Failed to initialize proxy state");
    return 1;
  }

  proxy_state->agents_registry =
      g_ptr_array_new_with_free_func(free_agent_callback_data);

  if (!connect_to_buses()) {
    cleanup_proxy_state();
    return 1;
  }

  if (!fetch_introspection_data()) {
    cleanup_proxy_state();
    return 1;
  }

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

  proxy_state->main_loop = g_main_loop_new(nullptr, FALSE);
  g_main_loop_run(proxy_state->main_loop);

  g_main_loop_unref(proxy_state->main_loop);
  cleanup_proxy_state();
  return 0;
}
