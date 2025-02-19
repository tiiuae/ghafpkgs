/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include "Methods.hpp"

#include <expected>
#include <string>

namespace ghaf::AudioControl::dbus
{

class Interface
{
public:
    using Ptr = std::shared_ptr<Interface>;

    Interface(std::string name, std::string objectPath);

    Interface& addMethod(BaseMethodPtr method);
    Interface& addSignal(BaseSignalPtr signal);

    std::variant<BaseMethod::MethodResult, Glib::Error> invokeMethod(std::string methodName, const Glib::ustring& sender,
                                                                     const BaseMethod::MethodParameters& parameters);

    const std::string& getName() const
    {
        return m_name;
    }

    const std::string& getObjectPath() const
    {
        return m_objectPath;
    }

    auto getSignals() const
    {
        return std::ranges::views::values(m_signals);
    }

private:
    std::string m_name;
    std::string m_objectPath;

    std::unordered_map<std::string, BaseMethodPtr> m_methods;
    std::unordered_map<std::string, BaseSignalPtr> m_signals;
};

} // namespace ghaf::AudioControl::dbus
