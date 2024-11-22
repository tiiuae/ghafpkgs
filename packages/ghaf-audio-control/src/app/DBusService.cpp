/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include "DBusService.hpp"

#include <GhafAudioControl/utils/Logger.hpp>

#include <giomm/dbusownname.h>

using namespace ghaf::AudioControl;

namespace
{

constexpr auto IntrospectionXml = "<node>"
                                  "  <interface name='org.ghaf.Audio'>"
                                  "    <method name='Open'/>"
                                  "  </interface>"
                                  "</node>";

}

DBusService::DBusService()
    : m_interfaceVtable(sigc::mem_fun(*this, &DBusService::onMethodCall))
    , m_introspectionData(Gio::DBus::NodeInfo::create_for_xml(IntrospectionXml))
    , m_connectionId(Gio::DBus::own_name(Gio::DBus::BUS_TYPE_SESSION, "org.ghaf.Audio", sigc::mem_fun(*this, &DBusService::onBusAcquired),
                                         sigc::mem_fun(*this, &DBusService::onNameAcquired), sigc::mem_fun(*this, &DBusService::onNameLost)))
{
}

DBusService::~DBusService()
{
    Gio::DBus::unown_name(m_connectionId);
}

void DBusService::onBusAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring&)
{
    Logger::debug("The bus is acquired, registering...");
    connection->register_object("/org/ghaf/Audio", m_introspectionData->lookup_interface("org.ghaf.Audio"), m_interfaceVtable);
}

void DBusService::onNameAcquired(const Glib::RefPtr<Gio::DBus::Connection>&, const Glib::ustring&)
{
    Logger::debug("The DBus service org.ghaf.Audio has been registered");
}

void DBusService::onNameLost(const Glib::RefPtr<Gio::DBus::Connection>&, const Glib::ustring&)
{
    Logger::error("Couldn't register service for org.ghaf.Audio");
}

void DBusService::onMethodCall([[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection, [[maybe_unused]] const Glib::ustring& sender,
                               [[maybe_unused]] const Glib::ustring& objectPath, [[maybe_unused]] const Glib::ustring& interfaceName,
                               const Glib::ustring& methodName, const Glib::VariantContainerBase& parameters,
                               const Glib::RefPtr<Gio::DBus::MethodInvocation>& invocation)
{
    Logger::debug("Invokated method: {}", methodName.c_str());

    if (methodName == "Open")
    {
        m_openSignal();
        invocation->return_value(parameters);
    }
}
