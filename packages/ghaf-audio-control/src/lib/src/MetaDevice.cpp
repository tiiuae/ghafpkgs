/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/MetaDevice.hpp>

#include <GhafAudioControl/utils/Check.hpp>
#include <GhafAudioControl/utils/Logger.hpp>

#include <ranges>

namespace ghaf::AudioControl
{

bool MetaDevice::operator==(const IDevice& other) const
{
    if (const auto* otherDevice = dynamic_cast<const MetaDevice*>(&other))
        return m_index == otherDevice->m_index;

    return false;
}

bool MetaDevice::isEnabled() const
{
    if (m_devices.empty())
        return false;

    return m_devices.begin()->second->isEnabled();
}

bool MetaDevice::isMuted() const
{
    return m_isMuted.value_or(false);
}

void MetaDevice::setMuted(bool mute)
{
    m_isMuted = mute;

    for (const auto& [_, device] : m_devices)
        device->setMuted(*m_isMuted);
}

Volume MetaDevice::getVolume() const
{
    return m_volume.value_or(Volume::fromPercents(0u));
}

void MetaDevice::setVolume(Volume volume)
{
    m_volume = volume;

    for (const auto& device : std::ranges::views::values(m_devices))
        device->setVolume(*m_volume);

    m_onUpdate();
}

std::string MetaDevice::toString() const
{
    return std::format("MetaDevice: index: {}, name: {}, isMuted: {}, volume: {}, devices: {}",
                       m_index,
                       m_name,
                       isMuted(),
                       getVolume().getPercents(),
                       m_devices.size());
}

MetaDevice::MetaDevice(Index index, std::string name)
    : m_index(index)
    , m_name(std::move(name))
{
}

void MetaDevice::addDevice(IDevice::Ptr device)
{
    CheckNullPtr(device);

    const auto index = device->getIndex();

    if (!m_devices.contains(index))
    {
        m_connections[index] = device->onDelete().connect([this, index] { deleteDevice(index); });

        if (m_isMuted)
            device->setMuted(*m_isMuted);
        else
            m_isMuted = device->isMuted();

        if (m_volume)
            device->setVolume(*m_volume);
        else
            m_volume = device->getVolume();

        m_devices.insert({index, std::move(device)});
    }
}

void MetaDevice::deleteDevice(Index index)
{
    m_devices.erase(index);
    m_connections.erase(index);
}

} // namespace ghaf::AudioControl
