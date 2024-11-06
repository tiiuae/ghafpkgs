/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <giomm/dbusinterfacevtable.h>
#include <giomm/dbusintrospection.h>
#include <giomm/dbusmethodinvocation.h>

class DBusService
{
public:
    using OpenSignalSignature = sigc::signal<void()>;

public:
    DBusService();
    ~DBusService();

public:
    OpenSignalSignature openSignal() const noexcept
    {
        return m_openSignal;
    }

private:
    void onBusAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);
    void onNameAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);
    void onNameLost(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);

    void onMethodCall(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& sender, const Glib::ustring& objectPath,
                      const Glib::ustring& interfaceName, const Glib::ustring& methodName, const Glib::VariantContainerBase& parameters,
                      const Glib::RefPtr<Gio::DBus::MethodInvocation>& invocation);

private:
    OpenSignalSignature m_openSignal;

    Gio::DBus::InterfaceVTable m_interfaceVtable;
    Glib::RefPtr<Gio::DBus::NodeInfo> m_introspectionData;

    guint m_connectionId;
};
