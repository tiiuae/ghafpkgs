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
#include "log.h"
#include "sni/proxy.h"

int main(int argc, gchar *argv[]) {
  // Default configuration
  ProxyConfig config = {.source_bus_name = nullptr,
                        .source_object_path = nullptr,
                        .proxy_bus_name = nullptr,
                        .source_bus_type = G_BUS_TYPE_SYSTEM,
                        .target_bus_type = G_BUS_TYPE_SESSION,
                        .sni_mode = FALSE,
                        .renamer_mode = FALSE,
                        .verbose = FALSE,
                        .info = FALSE};

  gchar *opt_source_bus_type = nullptr;
  gchar *opt_target_bus_type = nullptr;
  gchar *opt_log_level = nullptr;
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
      {"sni-mode", 0, 0, G_OPTION_ARG_NONE, &config.sni_mode,
       "Enable SNI (StatusNotifierItem) proxy mode", nullptr},
      {"renamer-mode", 0, 0, G_OPTION_ARG_NONE, &config.renamer_mode,
       "SNI renamer mode: register Watcher on local session bus and "
       "acquire well-known names for unique-name SNI apps (implies --sni-mode)",
       nullptr},
      {"log-level", 0, 0, G_OPTION_ARG_STRING, &opt_log_level,
       "Log level: verbose, info, error (default: info)", "LEVEL"},
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
    Log::error() << "Failed to parse options: " << error->message;
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
  if (opt_log_level) {
    if (g_strcmp0(opt_log_level, "verbose") == 0)
      Log::setLevel(Log::Level::Verbose);
    else if (g_strcmp0(opt_log_level, "error") == 0)
      Log::setLevel(Log::Level::Error);
    else if (g_strcmp0(opt_log_level, "info") != 0) {
      Log::error() << "Invalid --log-level value '" << opt_log_level
                   << "': must be verbose, info or error";
      g_free(opt_log_level);
      return 1;
    }
    g_free(opt_log_level);
  }

  if (config.renamer_mode) {
    // Renamer mode implies SNI mode; both buses are the local session bus
    config.sni_mode = TRUE;
    config.source_bus_type = G_BUS_TYPE_SESSION;
    config.target_bus_type = G_BUS_TYPE_SESSION;
    Log::info() << "Renamer mode enabled: registering Watcher on local session bus";
  }

  if (config.sni_mode) {
    // SNI mode: proxy dynamic StatusNotifierItem services
    if (!config.source_bus_name)
      config.source_bus_name = g_strdup(SNI_WATCHER_BUS_NAME);
    if (!config.source_object_path)
      config.source_object_path = g_strdup(SNI_WATCHER_OBJECT_PATH);
    if (!config.proxy_bus_name)
      config.proxy_bus_name = g_strdup(SNI_WATCHER_BUS_NAME);
  }

  if (!config.sni_mode) {
    // Ensure required parameters are provided
    if (!config.source_bus_name || config.source_bus_name[0] == '\0' ||
        !config.source_object_path || config.source_object_path[0] == '\0' ||
        !config.proxy_bus_name || config.proxy_bus_name[0] == '\0') {
      Log::error() << "Error: --source-bus-name, --source-object-path, and "
                      "--proxy-bus-name are required unless --sni-mode is used.";
      return 1;
    }
  }
  // Validate configuration
  validateProxyConfigOrExit(&config);

  Log::info() << "Starting cross-bus D-Bus proxy";
  Log::info() << "Source: " << config.source_bus_name << config.source_object_path << " on "
              << (config.source_bus_type == G_BUS_TYPE_SYSTEM ? "system" : "session") << " bus";
  Log::info() << "Target: " << config.proxy_bus_name << " on "
              << (config.target_bus_type == G_BUS_TYPE_SYSTEM ? "system" : "session") << " bus";

  // Initialize proxy state
  if (!init_proxy_state(&config)) {
    Log::error() << "Failed to initialize proxy state";
    return 1;
  }

  proxy_state->agents_registry =
      g_ptr_array_new_with_free_func(free_agent_callback_data);

  // Connect to both buses
  if (!connect_to_buses()) {
    cleanup_proxy_state();
    return 1;
  }

  // SNI mode: different initialization path
  if (proxy_state->config.sni_mode) {
    Log::info() << "Starting SNI proxy mode";
    auto sni = SniProxy::create(proxy_state->source_bus, proxy_state->target_bus,
                                proxy_state->config.target_bus_type,
                                proxy_state->config.renamer_mode);
    if (!sni) {
      Log::error() << "Failed to initialize SNI mode";
      cleanup_proxy_state();
      return 1;
    }

    proxy_state->main_loop = g_main_loop_new(nullptr, FALSE);
    g_main_loop_run(proxy_state->main_loop);

    g_main_loop_unref(proxy_state->main_loop);
    sni.reset(); // destroy SniProxy before bus connections are closed
    cleanup_proxy_state();
    return 0;
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
    Log::error() << "Failed to own name " << proxy_state->config.proxy_bus_name
                 << " on target bus";
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
    Log::error() << "Failed to watch name " << proxy_state->config.source_bus_name
                 << " on source bus";
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
