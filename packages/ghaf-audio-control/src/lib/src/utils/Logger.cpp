/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/utils/Logger.hpp>

#include <chrono>
#include <format>
#include <iostream>

namespace ghaf::AudioControl
{

void Logger::debug(std::string_view message)
{
    log(message, "debug");
}

void Logger::error(std::string_view message)
{
    log(message, "error");
}

void Logger::info(std::string_view message)
{
    log(message, "info");
}

void Logger::log(std::string_view message, std::string_view logLevel)
{
    const std::chrono::time_point timeNow = std::chrono::system_clock::now();

    if (logLevel == "error")
        std::cerr << "\033[31m";

    std::cerr << std::format("[{}] [{:5}] {}", timeNow, logLevel, message) << "\033[0m" << '\n';
}

} // namespace ghaf::AudioControl
