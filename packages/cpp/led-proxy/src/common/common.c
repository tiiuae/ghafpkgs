/*
 Copyright 2026 TII (CRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#define _GNU_SOURCE

#include <errno.h>
#include <limits.h>
#include <unistd.h>

// Read exactly len bytes from fd, retrying on EINTR and on short reads.
// Returns len on success, 0 on orderly shutdown before any data was read,
// or -1 on error/unexpected EOF.
ssize_t read_all(int fd, void *buf, size_t len) {
  size_t total = 0;
  while (total < len) {
    ssize_t n = read(fd, (char *)buf + total, len - total);
    if (n < 0) {
      if (errno == EINTR) {
        continue;
      }
      return -1;
    }
    if (n == 0) {
      return total == 0 ? 0 : -1; // EOF, possibly mid-message
    }
    total += (size_t)n;
  }
  return (ssize_t)total;
}

// Write exactly len bytes to fd, retrying on EINTR and on short writes.
// Returns len on success, -1 on error.
ssize_t write_all(int fd, const void *buf, size_t len) {
  size_t total = 0;
  while (total < len) {
    ssize_t n = write(fd, (const char *)buf + total, len - total);
    if (n < 0) {
      if (errno == EINTR) {
        continue;
      }
      return -1;
    }
    total += (size_t)n;
  }
  return (ssize_t)total;
}
