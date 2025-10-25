/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <format>
#include <string_view>

namespace ghaf::AudioControl
{

class Logger
{
private:
    enum class LogLevel
    {
        DEBUG,
        ERROR,
        INFO
    };

public:
    template<class... ArgsT>
    static void debug(std::string_view message, ArgsT... args)
    {
        log(std::vformat(message, std::make_format_args(args...)), LogLevel::DEBUG);
    }

    template<class... ArgsT>
    static void error(std::string_view message, ArgsT... args)
    {
        log(std::vformat(message, std::make_format_args(args...)), LogLevel::ERROR);
    }

    template<class... ArgsT>
    static void info(std::string_view message, ArgsT... args)
    {
        log(std::vformat(message, std::make_format_args(args...)), LogLevel::INFO);
    }

private:
    static std::string logLevelToString(LogLevel logLevel);
    static void log(std::string_view message, LogLevel logLevel);

    Logger();
};

} // namespace ghaf::AudioControl
