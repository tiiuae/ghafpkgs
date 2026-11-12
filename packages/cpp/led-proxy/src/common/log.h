/*
 Copyright 2026 TII (CRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#ifndef COMMON_LOG_H
#define COMMON_LOG_H

#include <stdio.h>

/* Error logging: always enabled, printed to stderr. */
#define LOG_ERROR(fmt, ...) fprintf(stderr, "[ERROR] " fmt "\n", ##__VA_ARGS__)

/* Debug logging: only active in debug builds (CMAKE_BUILD_TYPE=Debug
 * defines -DDEBUG), printed to stderr with the originating function
 * and line number. Compiled out entirely in release builds. */
#ifdef DEBUG
#define LOG_DEBUG(fmt, ...)                                                    \
  fprintf(stderr, "[DEBUG] %s:%d: " fmt "\n", __func__, __LINE__, ##__VA_ARGS__)
#else
#define LOG_DEBUG(fmt, ...)                                                    \
  do {                                                                         \
  } while (0)
#endif

#endif /* COMMON_LOG_H */
