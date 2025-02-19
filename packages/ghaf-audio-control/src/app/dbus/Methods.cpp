/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include "Methods.hpp"

#include <GhafAudioControl/utils/Logger.hpp>

#include <set>

using ghaf::AudioControl::Logger;

namespace ghaf::AudioControl::dbus
{

namespace
{

DeviceType IntToDeviceType(int value)
{
    switch (value)
    {
    case 0:
        return DeviceType::Sink;
    case 1:
        return DeviceType::Source;
    case 2:
        return DeviceType::SinkInput;
    case 3:
        return DeviceType::SourceOutput;
    case 4:
        return DeviceType::Meta;

    default:
        throw std::runtime_error{std::format("'type' field has an unsupported value: {}", value)};
    }
}

auto CreateEmptyResponse()
{
    return Glib::VariantContainerBase::create_tuple(std::vector<Glib::VariantBase>{});
};

auto CreateResultOkResponse()
{
    return Glib::VariantContainerBase::create_tuple(Glib::Variant<int>::create(0));
};

} // namespace

BaseMethod::BaseMethod(std::string name)
    : m_name(std::move(name))
{
}

SimpleMethod::SimpleMethod(std::string name)
    : BaseMethod(std::move(name))
{
}

BaseMethod::MethodResult SimpleMethod::invoke([[maybe_unused]] const Glib::ustring& sender, [[maybe_unused]] const MethodParameters& parameters)
{
    m_signal();
    return CreateEmptyResponse();
}

SubscribeToDeviceUpdatedSignalMethod::SubscribeToDeviceUpdatedSignalMethod()
    : BaseMethod("SubscribeToDeviceUpdatedSignal")
{
}

BaseMethod::MethodResult SubscribeToDeviceUpdatedSignalMethod::invoke(const Glib::ustring& sender, [[maybe_unused]] const MethodParameters& parameters)
{
    m_clients.add(sender);

    m_signal(sender);
    return CreateEmptyResponse();
}

void SubscribeToDeviceUpdatedSignalMethod::unsubscribe(const std::string& client)
{
    m_clients.remove(client);
}

UnsubscribeFromDeviceUpdatedSignalMethod::UnsubscribeFromDeviceUpdatedSignalMethod()
    : BaseMethod("UnsubscribeFromDeviceUpdatedSignal")
{
}

SetDeviceVolumeMethod::SetDeviceVolumeMethod()
    : BaseMethod("SetDeviceVolume")
{
}

BaseMethod::MethodResult UnsubscribeFromDeviceUpdatedSignalMethod::invoke(const Glib::ustring& sender, [[maybe_unused]] const MethodParameters& parameters)
{
    m_signal(sender);
    return CreateEmptyResponse();
}

SetDeviceMuteMethod::SetDeviceMuteMethod()
    : BaseMethod("SetDeviceMute")
{
}

BaseMethod::MethodResult SetDeviceVolumeMethod::invoke([[maybe_unused]] const Glib::ustring& sender, const MethodParameters& parameters)
{
    Glib::Variant<int> id;
    Glib::Variant<int> type;
    Glib::Variant<int> volume;

    parameters.get_child(id, 0);
    parameters.get_child(type, 1);
    parameters.get_child(volume, 2);

    m_signal(id.get(), IntToDeviceType(type.get()), Volume::fromPercents(static_cast<unsigned long>(volume.get())));

    return CreateResultOkResponse();
}

MakeDeviceDefaultMethod::MakeDeviceDefaultMethod()
    : BaseMethod("MakeDeviceDefault")
{
}

BaseMethod::MethodResult SetDeviceMuteMethod::invoke([[maybe_unused]] const Glib::ustring& sender, const MethodParameters& parameters)
{
    Glib::Variant<int> id;
    Glib::Variant<int> type;
    Glib::Variant<bool> mute;

    parameters.get_child(id, 0);
    parameters.get_child(type, 1);
    parameters.get_child(mute, 2);

    m_signal(id.get(), IntToDeviceType(type.get()), mute.get());

    return CreateResultOkResponse();
}

BaseMethod::MethodResult MakeDeviceDefaultMethod::invoke([[maybe_unused]] const Glib::ustring& sender, const MethodParameters& parameters)
{
    Glib::Variant<int> id;
    Glib::Variant<int> type;

    parameters.get_child(id, 0);
    parameters.get_child(type, 1);

    const auto deviceType = IntToDeviceType(type.get());

    if (!std::set{DeviceType::Sink, DeviceType::Source}.contains(deviceType))
        throw std::runtime_error{std::format("'type' field has an unsupported value: {}. Only Sink and Source allowed", type.get())};

    m_signal(id.get(), deviceType);

    return CreateResultOkResponse();
}

void SubscribeToDeviceUpdatedSignalMethod::Clients::add(const std::string& client)
{
    if (m_set.contains(client))
    {
        Logger::debug("DBusService: the client: {} already exists", client);
        return;
    }

    if (m_deque.size() == m_maxSize)
    {
        Logger::debug("DBusService: erasing the old client: {} as reached the limit of clients: {}", m_deque.front(), m_maxSize);

        const auto oldClient = m_deque.front();

        m_set.erase(m_deque.front());
        m_deque.pop_front();
        m_clientsChangeSignal(oldClient, ClientEvent::Delete);
    }

    Logger::debug("DBusService: add new client: {}", client);

    m_deque.push_back(client);
    m_set.insert(client);
    m_clientsChangeSignal(client, ClientEvent::Add);
}

void SubscribeToDeviceUpdatedSignalMethod::Clients::remove(const std::string& client)
{
    if (auto it = m_set.find(client); it != m_set.end())
    {
        m_deque.erase(std::find(m_deque.begin(), m_deque.end(), client));
        m_set.erase(it);

        Logger::debug("DBusService: deleted the client: {}", client);
    }
    else
        Logger::debug("DBusService: couldn't delete the client: {} as it doesn't exist", client);
}

DeviceUpdateSignal::DeviceUpdateSignal()
    : BaseSignal("DeviceUpdated")
{
}

void DeviceUpdateSignal::emit(DeviceIndex index, DeviceType type, const std::string& name, DeviceVolume volume, bool isMuted, bool isDefault,
                              DeviceEventType eventType, std::string destination)
{
    const Glib::VariantContainerBase args = Glib::VariantContainerBase::create_tuple({
        Glib::Variant<int>::create(index),                      // id
        Glib::Variant<int>::create(static_cast<int>(type)),     // type
        Glib::Variant<Glib::ustring>::create(name.c_str()),     // name
        Glib::Variant<int>::create(volume.getPercents()),       // volume
        Glib::Variant<bool>::create(isMuted),                   // isMuted
        Glib::Variant<bool>::create(isDefault),                 // isDefault
        Glib::Variant<int>::create(static_cast<int>(eventType)) // event
    });

    Logger::debug("DeviceUpdateSignal::emit: {} to the client: {}", args.print(true).c_str(), destination);
    doEmit(destination, args);
}

BaseSignal::BaseSignal(std::string name)
    : m_name(std::move(name))
{
}

OpenMethod::OpenMethod()
    : SimpleMethod("Open")
{
}

ToggleMethod::ToggleMethod()
    : SimpleMethod("Toggle")
{
}

ActivateMethod::ActivateMethod()
    : SimpleMethod("Activate")
{
}

} // namespace ghaf::AudioControl::dbus
