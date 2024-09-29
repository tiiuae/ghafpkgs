/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/SourceOutput.hpp>

#include <GhafAudioControl/Backends/PulseAudio/Helpers.hpp>
#include <GhafAudioControl/Backends/PulseAudio/Volume.hpp>

#include <format>

namespace ghaf::AudioControl::Backend::PulseAudio
{

SourceOutput::SourceOutput(const pa_source_output_info& info, pa_context& context)
    : m_device(info, context)
{
}

bool SourceOutput::operator==(const IDevice& other) const
{
    if (const auto* otherSource = dynamic_cast<const SourceOutput*>(&other))
        return m_device == otherSource->m_device;

    return false;
}

void SourceOutput::setMuted(bool mute)
{
    deleteCheck();
    ExecutePulseFunc(pa_context_set_source_output_mute, &m_device.getContext(), m_device.getIndex(), mute, nullptr, nullptr);
}

void SourceOutput::setVolume(Volume volume)
{
    deleteCheck();

    pa_cvolume paChannelVolume;
    std::ignore = pa_cvolume_set(&paChannelVolume, m_device.getPulseChannelVolume().channels, ToPulseAudioVolume(volume));

    ExecutePulseFunc(pa_context_set_source_output_volume, &m_device.getContext(), m_device.getIndex(), &paChannelVolume, nullptr, nullptr);
}

std::string SourceOutput::toString() const
{
    return std::format("PulseSourceOutput: [ {} ]", m_device.toString());
}

void SourceOutput::markDeleted()
{
    m_device.markDeleted();
    m_onDelete();
}

void SourceOutput::deleteCheck()
{
    if (m_device.isDeleted())
        throw std::logic_error{std::format("Using deleted device: {}", toString())};
}

} // namespace ghaf::AudioControl::Backend::PulseAudio
