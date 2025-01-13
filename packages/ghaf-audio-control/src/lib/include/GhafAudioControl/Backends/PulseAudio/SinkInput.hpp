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

class SinkInput final : public IAudioControlBackend::ISinkInput
{
public:
    SinkInput(const pa_sink_input_info& info, pa_context& context);

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
        return Type::SinkInput;
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

    uint32_t getCardIndex() const noexcept
    {
        return m_device.getCardIndex();
    }

    std::string toString() const override;

    std::optional<std::string> getAppVmName() const
    {
        return m_device.getAppVmName();
    }

    void update(const pa_sink_input_info& info);

    void markDeleted();

    OnUpdateSignal onUpdate() const override
    {
        return m_onUpdate;
    }

    OnDeleteSignal onDelete() const override
    {
        return m_onDelete;
    }

private:
    void deleteCheck();

private:
    GeneralDeviceImpl m_device;

    OnUpdateSignal m_onUpdate;
    OnDeleteSignal m_onDelete;
};

} // namespace ghaf::AudioControl::Backend::PulseAudio
