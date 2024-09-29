/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/SinkInput.hpp>

#include <GhafAudioControl/Backends/PulseAudio/Helpers.hpp>
#include <GhafAudioControl/Backends/PulseAudio/Volume.hpp>

#include <format>

namespace ghaf::AudioControl::Backend::PulseAudio
{

SinkInput::SinkInput(const pa_sink_input_info& info, pa_context& context)
    : m_device(info, context)
{
}

bool SinkInput::operator==(const IDevice& other) const
{
    if (const auto* otherSink = dynamic_cast<const SinkInput*>(&other))
        return m_device == otherSink->m_device;

    return false;
}

void SinkInput::setMuted(bool mute)
{
    deleteCheck();
    ExecutePulseFunc(pa_context_set_sink_input_mute, &m_device.getContext(), m_device.getIndex(), mute, nullptr, nullptr);
}

void SinkInput::setVolume(Volume volume)
{
    deleteCheck();

    pa_cvolume paChannelVolume;
    std::ignore = pa_cvolume_set(&paChannelVolume, m_device.getPulseChannelVolume().channels, ToPulseAudioVolume(volume));

    ExecutePulseFunc(pa_context_set_sink_input_volume, &m_device.getContext(), m_device.getIndex(), &paChannelVolume, nullptr, nullptr);
}

std::string SinkInput::toString() const
{
    return std::format("PulseSinkInput: [ {} ]", m_device.toString());
}

void SinkInput::update(const pa_sink_input_info& info)
{
    m_device.update(info);
    m_onUpdate();
}

void SinkInput::markDeleted()
{
    m_device.markDeleted();
    m_onDelete();
}

void SinkInput::deleteCheck()
{
    if (m_device.isDeleted())
        throw std::logic_error{std::format("Using deleted device: {}", toString())};
}

} // namespace ghaf::AudioControl::Backend::PulseAudio
