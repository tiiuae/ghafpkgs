/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <format>
#include <stdexcept>
#include <string>

namespace Details
{

template<class ExceptionT = std::runtime_error>
void Check(bool condition, const std::string& message, const char* conditionStr, size_t row, const char* filename, const char* functionName)
{
    if (condition)
        return;

    throw ExceptionT{
        std::format("Exception in file {} at row: {}\nFunction: {}\nCondition: {}\nMessage: {}", filename, row, functionName, conditionStr, message)};
}

template<class PtrT, class ExceptionT = std::invalid_argument>
void CheckNullPtr(PtrT&& ptr, const char* conditionStr, size_t row, const char* filename, const char* functionName)
{
    if (ptr)
        return;

    throw ExceptionT{std::format("Null pointer ({}) in file {} at row: {}\nFunction: {}\n", conditionStr, filename, row, functionName)};
}

} // namespace Details

#define Check(condition, message)                                                                \
    do                                                                                           \
    {                                                                                            \
        Details::Check(condition, message, #condition, __LINE__, __FILE__, __PRETTY_FUNCTION__); \
    } while (false);

#define CheckNullPtr(ptr) (Details::CheckNullPtr(ptr, #ptr, __LINE__, __FILE__, __PRETTY_FUNCTION__), ptr)
