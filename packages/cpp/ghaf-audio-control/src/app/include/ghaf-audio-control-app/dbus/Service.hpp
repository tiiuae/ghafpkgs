/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <ghaf-audio-control-app/dbus/Interface.hpp>

#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <giomm/dbusinterfacevtable.h>
#include <giomm/dbusintrospection.h>
#include <giomm/dbusmethodinvocation.h>

#include <optional>
#include <ranges>
#include <unordered_set>

namespace ghaf::AudioControl::dbus
{

class DBusService final
{
private:
    class Clients
    {
    private:
        std::deque<std::string> m_deque;
        std::unordered_set<std::string> m_set;
        const size_t m_maxSize = 50;

    public:
        void add(const std::string& client);
        void remove(const std::string& client);

        auto getList() const
        {
            return std::ranges::views::all(m_deque);
        }
    };

public:
    DBusService();
    ~DBusService();

public:
    void addInterface(Interface::Ptr interface);
    void registerSystemTrayIcon(const Glib::ustring& iconName);

private:
    void onBusAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);
    void onNameAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);
    void onNameLost(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);

    void onMethodCall(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& sender, const Glib::ustring& objectPath,
                      const Glib::ustring& interfaceName, const Glib::ustring& methodName, const Glib::VariantContainerBase& parameters,
                      const Glib::RefPtr<Gio::DBus::MethodInvocation>& invocation);

    void onPropertyGet(Glib::VariantBase& property, const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& sender,
                       const Glib::ustring& objectPath, const Glib::ustring& interface_name, const Glib::ustring& property_name);

    bool onPropertySet(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& sender, const Glib::ustring& objectPath,
                       const Glib::ustring& interfaceName, const Glib::ustring& propertyName, const Glib::VariantBase& value);

private:
    Gio::DBus::InterfaceVTable m_interfaceVtable;
    Glib::RefPtr<Gio::DBus::NodeInfo> m_introspectionData;

    std::map<std::string, Interface::Ptr> m_interfaces;
    std::optional<Glib::ustring> m_iconName;

    Glib::RefPtr<Gio::DBus::Connection> m_connection;

    guint m_connectionId;
};

} // namespace ghaf::AudioControl::dbus
