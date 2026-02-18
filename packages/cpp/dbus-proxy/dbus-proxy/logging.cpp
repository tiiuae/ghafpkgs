/*
 Copyright 2026 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "dbus_proxy.h"
#include <glib/gprintf.h>
#include <stdarg.h>

ProxyState *proxy_state = nullptr;

void log_verbose(const gchar *format, ...) {
  if (proxy_state && !proxy_state->config.verbose)
    return;

  va_list args;
  va_start(args, format);
  g_print("[VERBOSE] ");
  g_vprintf(format, args);
  g_print("\n");
  va_end(args);
}

void log_error(const gchar *format, ...) {
  va_list args;
  va_start(args, format);
  g_printerr("[ERROR] ");
  g_vfprintf(stderr, format, args);
  g_printerr("\n");
  va_end(args);
}

void log_info(const gchar *format, ...) {
  if (proxy_state && !proxy_state->config.info)
    return;

  va_list args;
  va_start(args, format);
  g_print("[INFO] ");
  g_vprintf(format, args);
  g_print("\n");
  va_end(args);
}
