/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <ghaf-audio-control-app/dbus/Interface.hpp>

#include <GhafAudioControl/utils/Check.hpp>

#include <giomm/dbuserror.h>

namespace ghaf::AudioControl::dbus
{

Interface::Interface(std::string name, std::string objectPath)
    : m_name(std::move(name))
    , m_objectPath(std::move(objectPath))
{
}

Interface& Interface::addMethod(BaseMethodPtr method)
{
    CheckNullPtr(method);

    m_methods[method->getName()] = std::move(method);
    return *this;
}

Interface& Interface::addSignal(BaseSignalPtr signal)
{
    CheckNullPtr(signal);

    m_signals[signal->getName()] = std::move(signal);
    return *this;
}

std::variant<BaseMethod::MethodResult, Glib::Error> Interface::invokeMethod(std::string methodName, const Glib::ustring& sender,
                                                                            const BaseMethod::MethodParameters& parameters)
{
    if (auto method = m_methods.find(methodName); method != m_methods.end())
        return method->second->invoke(sender, parameters);

    return Gio::DBus::Error(Gio::DBus::Error::FAILED, std::format("Interface: Unsupported method: {}", methodName));
}

} // namespace ghaf::AudioControl::dbus
