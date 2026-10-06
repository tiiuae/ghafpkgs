/*
 Copyright 2026 TII (CRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */
#ifndef COMMON_H
#define COMMON_H
#include <errno.h>
#include <limits.h>
#include <unistd.h>

ssize_t read_all(int fd, void *buf, size_t len);
ssize_t write_all(int fd, const void *buf, size_t len);

#endif // COMMON_H
