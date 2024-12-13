/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <giomm/dbusinterfacevtable.h>
#include <giomm/dbusintrospection.h>
#include <giomm/dbusmethodinvocation.h>

class DBusService final
{
public:
    using DeviceIndex = ghaf::AudioControl::IAudioControlBackend::IDevice::IntexT;
    using DeviceType = ghaf::AudioControl::IAudioControlBackend::IDevice::Type;
    using DeviceVolume = ghaf::AudioControl::Volume;
    using DeviceEventType = ghaf::AudioControl::IAudioControlBackend::EventType;

    using OpenSignalSignature = sigc::signal<void()>;
    using ToggleSignalSignature = sigc::signal<void()>;
    using SubscribeToDeviceUpdatedSignalSignature = sigc::signal<void()>;

    using SetDeviceVolumeSignalSignature = sigc::signal<void(DeviceIndex id, DeviceType type, DeviceVolume)>;
    using SetDeviceMuteSignalSignature = sigc::signal<void(DeviceIndex id, DeviceType type, bool mute)>;

    using MethodResult = Glib::VariantContainerBase;
    using MethodParameters = Glib::VariantContainerBase;
    using MethodCallHandler = std::function<MethodResult(const MethodParameters& parameters)>;

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

    SetDeviceVolumeSignalSignature setDeviceVolumeSignal() const noexcept
    {
        return m_setDeviceVolumeSignal;
    }

    SetDeviceMuteSignalSignature setDeviceMuteSignal() const noexcept
    {
        return m_setDeviceMuteSignal;
    }

    void sendDeviceInfo(DeviceIndex index, DeviceType type, const std::string& name, DeviceVolume volume, bool isMuted, DeviceEventType eventType);

private:
    void onBusAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);
    void onNameAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);
    void onNameLost(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name);

    void onMethodCall(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& sender, const Glib::ustring& objectPath,
                      const Glib::ustring& interfaceName, const Glib::ustring& methodName, const Glib::VariantContainerBase& parameters,
                      const Glib::RefPtr<Gio::DBus::MethodInvocation>& invocation);

    MethodResult onOpenMethod(const MethodParameters& parameters);
    MethodResult onToggleMethod(const MethodParameters& parameters);

    MethodResult onSubscribeToDeviceUpdatedSignalMethod(const MethodParameters& parameters);
    MethodResult onUnsubscribeFromDeviceUpdatedSignalMethod(const MethodParameters& parameters);

    MethodResult onSetDeviceVolumeMethod(const MethodParameters& parameters);
    MethodResult onSetDeviceMuteMethod(const MethodParameters& parameters);

private:
    OpenSignalSignature m_openSignal;
    ToggleSignalSignature m_toggleSignal;
    SubscribeToDeviceUpdatedSignalSignature m_subscribeToDeviceUpdatedSignal;

    SetDeviceVolumeSignalSignature m_setDeviceVolumeSignal;
    SetDeviceMuteSignalSignature m_setDeviceMuteSignal;

    Gio::DBus::InterfaceVTable m_interfaceVtable;
    Glib::RefPtr<Gio::DBus::NodeInfo> m_introspectionData;

    std::map<std::string, MethodCallHandler> m_methodHandlers;

    guint m_connectionId;
};
