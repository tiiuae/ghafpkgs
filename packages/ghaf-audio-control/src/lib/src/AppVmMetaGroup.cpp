/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/AppVmMetaGroup.hpp>
#include <GhafAudioControl/Backends/PulseAudio/SinkInput.hpp>

#include <GhafAudioControl/utils/Logger.hpp>

#include <format>
#include <string>

namespace ghaf::AudioControl
{

void MetaDeviceManager::sendDeviceInfoUpdate(const IAudioControlBackend::OnSignalMapChangeSignalInfo& info)
{
    const auto getMetaDevice = [this](Index internalDeviceIndex, std::optional<std::string> appVmName = std::nullopt) -> MetaDevice::Ptr
    {
        if (appVmName)
        {
            if (auto it = m_appVmMetaDeviceIndeces.find(appVmName.value()); it != m_appVmMetaDeviceIndeces.end())
                return m_metaDevices[it->second];
        }

        if (auto metaDeviceIt = m_internalDeviceIndexToMeta.find(internalDeviceIndex); metaDeviceIt != m_internalDeviceIndexToMeta.end())
            return metaDeviceIt->second;

        return nullptr;
    };

    const auto getAppVmName = [](const IAudioControlBackend::OnSignalMapChangeSignalInfo& info)
    {
        return std::dynamic_pointer_cast<Backend::PulseAudio::SinkInput>(info.ptr)->getAppVmName().value_or("AppVMs");
    };

    IAudioControlBackend::OnSignalMapChangeSignalInfo metaDeviceInfo;
    metaDeviceInfo.type = IAudioControlBackend::IDevice::Type::Meta;

    switch (info.eventType)
    {
    case IAudioControlBackend::EventType::Add:
    {
        auto sinkInput = std::dynamic_pointer_cast<Backend::PulseAudio::SinkInput>(info.ptr);
        const std::string appVmName = getAppVmName(info);

        if (auto metaDevice = getMetaDevice(info.index, appVmName))
        {
            metaDevice->addDevice(std::move(sinkInput));
            m_internalDeviceIndexToMeta[info.index] = metaDevice;

            metaDeviceInfo.eventType = IAudioControlBackend::EventType::Update;
            metaDeviceInfo.index = metaDevice->getIndex();
            metaDeviceInfo.ptr = metaDevice;
        }
        else
        {
            const Index appVmIndex = [this, &appVmName]()
            {
                if (auto it = m_appVmMetaDeviceIndeces.find(appVmName); it != m_appVmMetaDeviceIndeces.end())
                    return it->second;

                m_names.push_back(appVmName);

                const Index newIndex = m_names.size() - 1;
                m_appVmMetaDeviceIndeces[appVmName] = newIndex;

                return newIndex;
            }();

            auto [newMetaDeviceIt, success] = m_metaDevices.emplace(appVmIndex, std::make_shared<MetaDevice>(appVmIndex, appVmName));
            auto newMetaDevice = newMetaDeviceIt->second;

            if (success)
                newMetaDevice->addDevice(std::move(sinkInput));

            m_internalDeviceIndexToMeta[info.index] = newMetaDevice;

            metaDeviceInfo.eventType = IAudioControlBackend::EventType::Add;
            metaDeviceInfo.index = newMetaDevice->getIndex();
            metaDeviceInfo.ptr = newMetaDevice;
        }

        m_onUpdate(metaDeviceInfo);

        break;
    }

    case IAudioControlBackend::EventType::Update:
    {
        if (auto metaDevice = getMetaDevice(info.index))
        {
            metaDeviceInfo.eventType = IAudioControlBackend::EventType::Update;
            metaDeviceInfo.index = metaDevice->getIndex();
            metaDeviceInfo.ptr = metaDevice;
        }

        m_onUpdate(metaDeviceInfo);

        break;
    }

    case IAudioControlBackend::EventType::Delete:
    {
        if (auto metaDevice = getMetaDevice(info.index))
        {
            metaDeviceInfo.eventType = IAudioControlBackend::EventType::Update;
            metaDeviceInfo.index = metaDevice->getIndex();
            metaDeviceInfo.ptr = metaDevice;
        }

        m_onUpdate(metaDeviceInfo);

        break;
    }
    }
}

void MetaDeviceManager::setDeviceVolume(IAudioControlBackend::IDevice::IntexT metaIndex, Volume volume)
{
    if (auto it = m_metaDevices.find(metaIndex); it != m_metaDevices.end())
        it->second->setVolume(volume);
    else
        Logger::error("MetaDeviceManager::setDeviceVolume: device with index: {} wasn't found", metaIndex);
}

void MetaDeviceManager::setDeviceMute(IAudioControlBackend::IDevice::IntexT metaIndex, bool mute)
{
    if (auto it = m_metaDevices.find(metaIndex); it != m_metaDevices.end())
        it->second->setMuted(mute);
    else
        Logger::error("MetaDeviceManager::setDeviceMute: device with index: {} wasn't found", metaIndex);
}

} // namespace ghaf::AudioControl
