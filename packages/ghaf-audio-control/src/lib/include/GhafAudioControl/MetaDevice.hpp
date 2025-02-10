/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <sigc++/connection.h>

#include <string>

namespace ghaf::AudioControl
{

class MetaDevice : public IAudioControlBackend::IDevice
{
public:
    using Ptr = std::shared_ptr<MetaDevice>;

    MetaDevice(Index index, std::string name);

    void addDevice(IDevice::Ptr device);
    void deleteDevice(Index index);

    bool operator==(const IDevice& other) const override;

    [[nodiscard]] Index getIndex() const override
    {
        return m_index;
    }

    [[nodiscard]] std::string getName() const override
    {
        return m_name;
    }

    [[nodiscard]] Type getType() const override
    {
        return Type::Meta;
    }

    [[nodiscard]] bool isEnabled() const override;

    [[nodiscard]] bool isMuted() const override;

    void setMuted(bool mute) override;

    [[nodiscard]] Volume getVolume() const override;

    void setVolume(Volume volume) override;

    [[nodiscard]] uint32_t getCardIndex() const noexcept
    {
        return 0;
    }

    [[nodiscard]] std::string toString() const override;

    [[nodiscard]] std::string getDescription() const override
    {
        return m_name;
    }

    [[nodiscard]] OnUpdateSignal onUpdate() const override
    {
        return m_onUpdate;
    }

    [[nodiscard]] OnDeleteSignal onDelete() const override
    {
        return m_onDelete;
    }

private:
    Index m_index;
    std::string m_name;

    std::map<Index, IDevice::Ptr> m_devices;
    std::map<Index, sigc::connection> m_connections;

    OnUpdateSignal m_onUpdate;
    OnDeleteSignal m_onDelete;
};

} // namespace ghaf::AudioControl
