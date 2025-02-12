/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <giomm/dbusinterfacevtable.h>
#include <giomm/dbusintrospection.h>
#include <giomm/dbusmethodinvocation.h>

#include <ranges>
#include <unordered_set>

class DBusService final
{
public:
    using DeviceIndex = ghaf::AudioControl::IAudioControlBackend::IDevice::IntexT;
    using DeviceType = ghaf::AudioControl::IAudioControlBackend::IDevice::Type;
    using DeviceVolume = ghaf::AudioControl::Volume;
    using DeviceEventType = ghaf::AudioControl::IAudioControlBackend::EventType;

    using OpenSignalSignature = sigc::signal<void()>;
    using ToggleSignalSignature = sigc::signal<void()>;
    using SubscribeToDeviceUpdatedSignalSignature = sigc::signal<void(const Glib::ustring& name)>;
    using UnsubscribeToDeviceUpdatedSignalSignature = sigc::signal<void()>;

    using SetDeviceVolumeSignalSignature = sigc::signal<void(DeviceIndex id, DeviceType type, DeviceVolume)>;
    using SetDeviceMuteSignalSignature = sigc::signal<void(DeviceIndex id, DeviceType type, bool mute)>;

    using MakeDeviceDefaultSignalSignature = sigc::signal<void(DeviceIndex id, DeviceType type)>;

    using MethodResult = Glib::VariantContainerBase;
    using MethodParameters = Glib::VariantContainerBase;
    using MethodCallHandler = std::function<MethodResult(const Glib::ustring& sender, const MethodParameters& parameters)>;

private:
    class Clients
    {
    private:
        std::deque<std::string> m_deque;
        std::unordered_set<std::string> m_set;
        size_t m_maxSize = 50;

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
    OpenSignalSignature openSignal() const noexcept
    {
        return m_openSignal;
    }

    ToggleSignalSignature toggleSignal() const noexcept
    {
        return m_toggleSignal;
    }

    SubscribeToDeviceUpdatedSignalSignature subscribeToDeviceUpdatedSignal() const noexcept
    {
        return m_subscribeToDeviceUpdatedSignal;
    }

    UnsubscribeToDeviceUpdatedSignalSignature unsubscribeToDeviceUpdatedSignal() const noexcept
    {
        return m_unsubscribeToDeviceUpdatedSignal;
    }

    SetDeviceVolumeSignalSignature setDeviceVolumeSignal() const noexcept
    {
        return m_setDeviceVolumeSignal;
    }

    SetDeviceMuteSignalSignature setDeviceMuteSignal() const noexcept
    {
        return m_setDeviceMuteSignal;
    }

    MakeDeviceDefaultSignalSignature makeDeviceDefaultSignal() const noexcept
    {
        return m_makeDeviceDefaultSignal;
    }

    void sendDeviceInfo(DeviceIndex index, DeviceType type, const std::string& name, DeviceVolume volume, bool isMuted, bool isDefault,
                        DeviceEventType eventType, std::optional<std::string> destination = std::nullopt);
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

    MethodResult onOpenMethod(const Glib::ustring& sender, const MethodParameters& parameters);
    MethodResult onToggleMethod(const Glib::ustring& sender, const MethodParameters& parameters);

    MethodResult onSubscribeToDeviceUpdatedSignalMethod(const Glib::ustring& sender, const MethodParameters& parameters);
    MethodResult onUnsubscribeFromDeviceUpdatedSignalMethod(const Glib::ustring& sender, const MethodParameters& parameters);

    MethodResult onSetDeviceVolumeMethod(const Glib::ustring& sender, const MethodParameters& parameters);
    MethodResult onSetDeviceMuteMethod(const Glib::ustring& sender, const MethodParameters& parameters);

    MethodResult onMakeDeviceDefaultMethod(const Glib::ustring& sender, const MethodParameters& parameters);

    MethodResult onActivateMethod(const Glib::ustring& sender, const MethodParameters& parameters);

private:
    OpenSignalSignature m_openSignal;
    ToggleSignalSignature m_toggleSignal;
    SubscribeToDeviceUpdatedSignalSignature m_subscribeToDeviceUpdatedSignal;
    UnsubscribeToDeviceUpdatedSignalSignature m_unsubscribeToDeviceUpdatedSignal;

    SetDeviceVolumeSignalSignature m_setDeviceVolumeSignal;
    SetDeviceMuteSignalSignature m_setDeviceMuteSignal;

    MakeDeviceDefaultSignalSignature m_makeDeviceDefaultSignal;

    Gio::DBus::InterfaceVTable m_interfaceVtable;
    Glib::RefPtr<Gio::DBus::NodeInfo> m_introspectionData;

    std::map<std::string, MethodCallHandler> m_audioControlServiceMethodHandlers;
    std::map<std::string, MethodCallHandler> m_statusNotifierItemMethodHandlers;

    Clients m_subscribedClients;

    std::optional<Glib::ustring> m_iconName;

    Glib::RefPtr<Gio::DBus::Connection> m_connection;

    guint m_connectionId;
};
