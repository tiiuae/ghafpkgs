/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/models/DeviceModel.hpp>

#include <GhafAudioControl/utils/Logger.hpp>

#include <glibmm/main.h>

namespace ghaf::AudioControl
{

namespace
{

constexpr auto CheckMarkSymbol = "✔";

template<class T>
void LazySet(Glib::Property<T>& property, const typename Glib::Property<T>::PropertyType& newValue)
{
    if (property == newValue)
        return;

    property = newValue;
}

auto Bind(const auto& appProp, const auto& widgetProp, bool readonly = false)
{
    auto flag = Glib::BindingFlags::BINDING_SYNC_CREATE;
    if (!readonly)
        flag |= Glib::BindingFlags::BINDING_BIDIRECTIONAL;

    return Glib::Binding::bind_property(appProp, widgetProp, flag);
}

auto GetDeviceName(const IAudioControlBackend::IDevice& device)
{
    bool isDefault = false;

    if (const auto* defaultable = dynamic_cast<const IAudioControlBackend::IDefaultable*>(&device))
        isDefault = defaultable->isDefault();

    // Set description as a name for sinks and sources -- as it's less ugly
    auto name = (device.getType() == IAudioControlBackend::IDevice::Type::Sink || device.getType() == IAudioControlBackend::IDevice::Type::Source)
                    ? device.getDescription()
                    : device.getName();

    if (isDefault)
        return CheckMarkSymbol + name;

    return "   " + name;
}

} // namespace

DeviceModel::DeviceModel(IAudioControlBackend::IDevice::Ptr device)
    : Glib::ObjectBase(typeid(DeviceModel))
    , m_device(std::move(device))
    , m_isEnabled{*this, "m_isEnabled", false}
    , m_connections{m_isDefault.get_proxy().signal_changed().connect(sigc::mem_fun(*this, &DeviceModel::onDefaultChange)),
                    m_isSoundEnabled.get_proxy().signal_changed().connect(sigc::mem_fun(*this, &DeviceModel::onSoundEnabledChange)),
                    m_soundVolume.get_proxy().signal_changed().connect(sigc::mem_fun(*this, &DeviceModel::onSoundVolumeChange)),
                    m_device->onUpdate().connect([this] { updateDevice(); })}
{
    updateDevice();
}

Glib::RefPtr<DeviceModel> DeviceModel::create(IAudioControlBackend::IDevice::Ptr device)
{
    return Glib::RefPtr<DeviceModel>(new DeviceModel(std::move(device)));
}

int DeviceModel::compare(const Glib::RefPtr<const DeviceModel>& a, const Glib::RefPtr<const DeviceModel>& b)
{
    if (!a || !b)
        return 0;

    return 1;
}

void DeviceModel::updateDevice()
{
    bool isDefault = false;

    if (auto* defaultable = dynamic_cast<IAudioControlBackend::IDefaultable*>(m_device.get()))
        isDefault = defaultable->isDefault();

    {
        const auto scopeExit = m_connections.blockGuarded();

        LazySet(m_isSoundEnabled, !m_device->isMuted());
        LazySet(m_soundVolume, m_device->getVolume().getPercents());
        LazySet(m_isDefault, isDefault);
    }

    LazySet(m_name, GetDeviceName(*m_device));
}

void DeviceModel::onDefaultChange()
{
    const auto isDefault = m_isDefault.get_value();

    Logger::debug("Default has changed to: {}", isDefault);

    if (auto* defaultable = dynamic_cast<IAudioControlBackend::IDefaultable*>(m_device.get()))
        defaultable->setDefault(isDefault);
}

void DeviceModel::onSoundEnabledChange()
{
    const auto isEnabled = m_isSoundEnabled.get_value();

    Logger::debug("SoundEnabled has changed to: {}", isEnabled);
    m_device->setMuted(!isEnabled);
}

void DeviceModel::onSoundVolumeChange()
{
    Logger::debug("SoundVolume has changed to: {}", m_soundVolume.get_value());
    m_device->setVolume(Volume::fromPercents(m_soundVolume.get_value()));
}

} // namespace ghaf::AudioControl
