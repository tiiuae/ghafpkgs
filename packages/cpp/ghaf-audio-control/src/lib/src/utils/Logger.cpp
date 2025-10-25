/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/utils/Logger.hpp>

#include <chrono>
#include <format>
#include <iostream>

namespace ghaf::AudioControl
{

std::string Logger::logLevelToString(LogLevel logLevel)
{
    if (logLevel == Logger::LogLevel::DEBUG)
        return "debug";

    if (logLevel == Logger::LogLevel::ERROR)
        return "error";

    if (logLevel == Logger::LogLevel::INFO)
        return "info";

    return "unknown";
}

void Logger::log(std::string_view message, LogLevel logLevel)
{
    const std::chrono::time_point timeNow = std::chrono::system_clock::now();

    if (logLevel == Logger::LogLevel::ERROR)
        std::cerr << "\033[31m";

    std::cerr << std::format("[{}] [{:5}] {}", timeNow, logLevelToString(logLevel), message) << "\033[0m" << '\n';
}

} // namespace ghaf::AudioControl
