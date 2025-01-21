/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/Backends/PulseAudio/GeneralDevice.hpp>
#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <pulse/context.h>
#include <pulse/introspect.h>

namespace ghaf::AudioControl::Backend::PulseAudio
{

class Source final : public IAudioControlBackend::ISource
{
public:
    Source(const pa_source_info& info, bool isDefault, pa_context& context);

    bool operator==(const IDevice& other) const override;

    Index getIndex() const override
    {
        return m_device.getIndex();
    }

    std::string getName() const override
    {
        return m_device.getName();
    }

    [[nodiscard]] std::string getDescription() const override
    {
        return m_device.getDescription();
    }

    Type getType() const override
    {
        return Type::Source;
    }

    bool isEnabled() const override
    {
        return m_device.isEnabled();
    }

    bool isMuted() const override
    {
        return m_device.isMuted();
    }

    void setMuted(bool mute) override;

    Volume getVolume() const override
    {
        return m_device.getVolume();
    }

    void setVolume(Volume volume) override;

    std::string toString() const override;

    uint32_t getCardIndex() const;

    void update(const pa_source_info& info);
    void update(const pa_card_info& info);

    void markDeleted();

    OnUpdateSignal onUpdate() const override
    {
        return m_onUpdate;
    }

    OnDeleteSignal onDelete() const override
    {
        return m_onDelete;
    }

    void setDefault(bool value) override;

    [[nodiscard]] bool isDefault() const override;

    void updateDefault(bool value) override;

private:
    void deleteCheck();

private:
    GeneralDeviceImpl m_device;

    OnUpdateSignal m_onUpdate;
    OnDeleteSignal m_onDelete;
};

} // namespace ghaf::AudioControl::Backend::PulseAudio
