/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include "GhafAudioControl/utils/ConnectionContainer.hpp"
#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <GhafAudioControl/MetaDevice.hpp>

#include <string>

namespace ghaf::AudioControl
{

class MetaDeviceManager
{
public:
    MetaDeviceManager() = default;

    MetaDeviceManager(const MetaDeviceManager&) = default;
    MetaDeviceManager(MetaDeviceManager&&) = default;

    MetaDeviceManager& operator=(const MetaDeviceManager&) = default;
    MetaDeviceManager& operator=(MetaDeviceManager&&) = default;

    void sendDeviceInfoUpdate(const IAudioControlBackend::OnSignalMapChangeSignalInfo& info);

    void setDeviceVolume(IAudioControlBackend::IDevice::IntexT metaIndex, Volume volume);
    void setDeviceMute(IAudioControlBackend::IDevice::IntexT metaIndex, bool mute);

    IAudioControlBackend::OnSignalMapChangeSignalInfo::Signal onDeviceUpdateSignal()
    {
        return m_onUpdate;
    }

private:
    std::map<Index, MetaDevice::Ptr> m_metaDevices;
    std::map<Index, MetaDevice::Ptr> m_internalDeviceIndexToMeta;

    std::vector<std::string> m_names;
    std::map<std::string, Index> m_appVmMetaDeviceIndeces;
    IAudioControlBackend::OnSignalMapChangeSignalInfo::Signal m_onUpdate;

    ConnectionContainer m_connections;
};

} // namespace ghaf::AudioControl
