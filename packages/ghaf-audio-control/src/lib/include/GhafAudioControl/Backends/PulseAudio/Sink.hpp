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

class Sink final : public IAudioControlBackend::ISink
{
public:
    Sink(const pa_sink_info& info, bool isDefault, pa_context& context);

    bool operator==(const IDevice& other) const override;

    [[nodiscard]] Index getIndex() const override
    {
        return m_device.getIndex();
    }

    [[nodiscard]] std::string getName() const override
    {
        return m_device.getName();
    }

    [[nodiscard]] Type getType() const override
    {
        return Type::Sink;
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

    [[nodiscard]] Volume getVolume() const override
    {
        return m_device.getVolume();
    }

    void setVolume(Volume volume) override;

    [[nodiscard]] uint32_t getCardIndex() const noexcept
    {
        return m_device.getCardIndex();
    }

    [[nodiscard]] std::string toString() const override;

    [[nodiscard]] std::string getDescription() const override
    {
        return m_device.getDescription();
    }

    void update(const pa_sink_info& info);
    void update(const pa_card_info& info);

    void markDeleted();

    [[nodiscard]] OnUpdateSignal onUpdate() const override
    {
        return m_onUpdate;
    }

    [[nodiscard]] OnDeleteSignal onDelete() const override
    {
        return m_onDelete;
    }

    void setDefault(bool value) override;

    [[nodiscard]] bool isDefault() const override
    {
        return m_device.isDefault();
    }

    void updateDefault(bool value) override;

private:
    void deleteCheck();

private:
    GeneralDeviceImpl m_device;

    OnUpdateSignal m_onUpdate;
    OnDeleteSignal m_onDelete;
};

} // namespace ghaf::AudioControl::Backend::PulseAudio
