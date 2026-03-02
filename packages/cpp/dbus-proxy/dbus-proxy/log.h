/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <glib.h>
#include <sstream>
#include <utility>

// RAII log line: accumulates via <<, flushes to g_print/g_printerr on destroy.
// Level::Noop produces no output and skips ostringstream writes.
struct LogStream {
    enum class Level { Noop, Verbose, Info, Error };

    explicit LogStream(Level l) : level_(l) {}
    ~LogStream() {
        if (level_ == Level::Noop)
            return;
        auto msg = oss_.str();
        switch (level_) {
        case Level::Verbose:
            g_print("[VERBOSE] %s\n", msg.c_str());
            break;
        case Level::Info:
            g_print("[INFO] %s\n", msg.c_str());
            break;
        case Level::Error:
            g_printerr("[ERROR] %s\n", msg.c_str());
            break;
        default:
            break;
        }
    }
    LogStream(const LogStream&) = delete;
    LogStream& operator=(const LogStream&) = delete;

    template <typename T> LogStream& operator<<(T&& val) {
        if (level_ != Level::Noop)
            oss_ << std::forward<T>(val);
        return *this;
    }

  private:
    Level level_;
    std::ostringstream oss_;
};

// Singleton logger with runtime level control.
// Default level is Info; call Log::setLevel(Level::Verbose) to enable
// verbose output (e.g. when --verbose flag is passed on the command line).
class Log {
  public:
    enum class Level { Verbose, Info, Error };

    static Log& instance() {
        static Log s;
        return s;
    }

    Log(const Log&) = delete;
    Log& operator=(const Log&) = delete;

    static void setLevel(Level l) { instance().log_level_ = l; }

    static LogStream verbose() {
        if (instance().log_level_ > Level::Verbose)
            return LogStream{LogStream::Level::Noop};
        return LogStream{LogStream::Level::Verbose};
    }
    static LogStream info() {
        if (instance().log_level_ > Level::Info)
            return LogStream{LogStream::Level::Noop};
        return LogStream{LogStream::Level::Info};
    }
    static LogStream error() { return LogStream{LogStream::Level::Error}; }

  private:
    Log() = default;
    ~Log() = default;
    Level log_level_ = Level::Info;
};
