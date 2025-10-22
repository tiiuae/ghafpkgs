/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/Volume.hpp>

#include <glibmm/variant.h>
#include <sigc++/signal.h>

#include <deque>
#include <functional>
#include <memory>
#include <ranges>
#include <unordered_set>

namespace ghaf::AudioControl::dbus
{

using DeviceIndex = ghaf::AudioControl::IAudioControlBackend::IDevice::IntexT;
using DeviceType = ghaf::AudioControl::IAudioControlBackend::IDevice::Type;
using DeviceVolume = ghaf::AudioControl::Volume;
using DeviceEventType = ghaf::AudioControl::IAudioControlBackend::EventType;

class BaseMethod
{
public:
    using MethodResult = Glib::VariantContainerBase;
    using MethodParameters = Glib::VariantContainerBase;
    using MethodCallHandler = std::function<MethodResult(const Glib::ustring& sender, const MethodParameters& parameters)>;

    explicit BaseMethod(std::string name);
    virtual ~BaseMethod() = default;

    [[nodiscard]] const std::string& getName() const
    {
        return m_name;
    }

    virtual MethodResult invoke([[maybe_unused]] const Glib::ustring& sender, const MethodParameters& parameters) = 0;

private:
    std::string m_name;
};
using BaseMethodPtr = std::shared_ptr<BaseMethod>;

class BaseSignal
{
public:
    using MethodParameters = Glib::VariantContainerBase;
    using SignalSignature = sigc::signal<void(const Glib::ustring& destination, const MethodParameters& parameters)>;

    explicit BaseSignal(std::string name);
    virtual ~BaseSignal() = default;

    [[nodiscard]] const std::string& getName() const
    {
        return m_name;
    }

    SignalSignature onSignal() const
    {
        return m_signal;
    }

protected:
    void doEmit(const Glib::ustring& destination, const MethodParameters& parameters)
    {
        m_signal(destination, parameters);
    }

private:
    std::string m_name;
    SignalSignature m_signal;
};
using BaseSignalPtr = std::shared_ptr<BaseSignal>;

class DeviceUpdateSignal : public BaseSignal
{
public:
    DeviceUpdateSignal();

    void emit(DeviceIndex index, DeviceType type, const std::string& name, DeviceVolume volume, bool isMuted, bool isDefault, DeviceEventType eventType,
              std::string destination);
};

class SimpleMethod : public BaseMethod
{
public:
    using SignalSignature = sigc::signal<void()>;

    explicit SimpleMethod(std::string name);

    MethodResult invoke(const Glib::ustring& sender, const MethodParameters& parameters) override;

    SignalSignature onInvokation() const
    {
        return m_signal;
    }

private:
    SignalSignature m_signal;
};

class OpenMethod : public SimpleMethod
{
public:
    OpenMethod();
};

class ToggleMethod : public SimpleMethod
{
public:
    ToggleMethod();
};

class ActivateMethod : public SimpleMethod
{
public:
    ActivateMethod();
};

class SubscribeToDeviceUpdatedSignalMethod : public BaseMethod
{
private:
    class Clients
    {
    public:
        enum class ClientEvent
        {
            Add,
            Delete
        };

        using SignalSignature = sigc::signal<void(const std::string& client, ClientEvent event)>;

        void add(const std::string& client);
        void remove(const std::string& client);

        auto getList() const
        {
            return std::ranges::views::all(m_deque);
        }

        SignalSignature onClientsChange() const
        {
            return m_clientsChangeSignal;
        }

    private:
        std::deque<std::string> m_deque;
        std::unordered_set<std::string> m_set;
        const size_t m_maxSize = 50;

        SignalSignature m_clientsChangeSignal;
    };

public:
    using SignalSignature = sigc::signal<void(const Glib::ustring& sender)>;

    SubscribeToDeviceUpdatedSignalMethod();

    MethodResult invoke(const Glib::ustring& sender, const MethodParameters& parameters) override;
    void unsubscribe(const std::string& client);

    auto getClientList() const
    {
        return m_clients.getList();
    }

    SignalSignature onInvokation() const
    {
        return m_signal;
    }

private:
    Clients m_clients;
    SignalSignature m_signal;
};

class UnsubscribeFromDeviceUpdatedSignalMethod : public BaseMethod
{
public:
    using SignalSignature = sigc::signal<void(const Glib::ustring& senderName)>;

    UnsubscribeFromDeviceUpdatedSignalMethod();

    MethodResult invoke(const Glib::ustring& sender, const MethodParameters& parameters) override;

    SignalSignature onInvokation() const
    {
        return m_signal;
    }

private:
    SignalSignature m_signal;
};

class SetDeviceVolumeMethod : public BaseMethod
{
public:
    using SignalSignature = sigc::signal<void(ghaf::AudioControl::IAudioControlBackend::IDevice::IntexT id,
                                              ghaf::AudioControl::IAudioControlBackend::IDevice::Type type, ghaf::AudioControl::Volume)>;

    SetDeviceVolumeMethod();

    MethodResult invoke(const Glib::ustring& sender, const MethodParameters& parameters) override;

    SignalSignature onInvokation() const
    {
        return m_signal;
    }

private:
    SignalSignature m_signal;
};

class SetDeviceMuteMethod : public BaseMethod
{
public:
    using SignalSignature = sigc::signal<void(ghaf::AudioControl::IAudioControlBackend::IDevice::IntexT id,
                                              ghaf::AudioControl::IAudioControlBackend::IDevice::Type type, bool mute)>;

    SetDeviceMuteMethod();

    MethodResult invoke(const Glib::ustring& sender, const MethodParameters& parameters) override;

    SignalSignature onInvokation() const
    {
        return m_signal;
    }

private:
    SignalSignature m_signal;
};

class MakeDeviceDefaultMethod : public BaseMethod
{
public:
    using SignalSignature
        = sigc::signal<void(ghaf::AudioControl::IAudioControlBackend::IDevice::IntexT id, ghaf::AudioControl::IAudioControlBackend::IDevice::Type type)>;

    MakeDeviceDefaultMethod();

    MethodResult invoke(const Glib::ustring& sender, const MethodParameters& parameters) override;

    SignalSignature onInvokation() const
    {
        return m_signal;
    }

private:
    SignalSignature m_signal;
};

} // namespace ghaf::AudioControl::dbus
