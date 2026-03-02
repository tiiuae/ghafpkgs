/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <glib.h>
#include <sstream>
#include <utility>

// RAII log line: accumulates via <<, flushes to g_print/g_printerr on destroy.
struct SniLogStream {
  enum class Level { Verbose, Info, Error };

  explicit SniLogStream(Level l) : level_(l) {}
  ~SniLogStream() {
    auto msg = oss_.str();
    switch (level_) {
    case Level::Verbose:
      g_print("[SNI VERBOSE] %s\n", msg.c_str());
      break;
    case Level::Info:
      g_print("[SNI INFO] %s\n", msg.c_str());
      break;
    case Level::Error:
      g_printerr("[SNI ERROR] %s\n", msg.c_str());
      break;
    }
  }
  SniLogStream(const SniLogStream &) = delete;
  SniLogStream &operator=(const SniLogStream &) = delete;

  template <typename T> SniLogStream &operator<<(T &&val) {
    oss_ << std::forward<T>(val);
    return *this;
  }

private:
  Level level_;
  std::ostringstream oss_;
};

// Meyer's singleton logger.
// Instance is created on first call to get() and lives for the program
// lifetime.
class SniLog {
public:
  static SniLog &instance() {
    static SniLog s;
    return s;
  }

  SniLog(const SniLog &) = delete;
  SniLog &operator=(const SniLog &) = delete;

  static SniLogStream verbose() {
    return SniLogStream{SniLogStream::Level::Verbose};
  }
  static SniLogStream info() { return SniLogStream{SniLogStream::Level::Info}; }
  static SniLogStream error() {
    return SniLogStream{SniLogStream::Level::Error};
  }

private:
  SniLog() = default;
  ~SniLog() = default;
};
