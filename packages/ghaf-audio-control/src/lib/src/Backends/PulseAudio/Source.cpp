/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/Source.hpp>

#include <GhafAudioControl/Backends/PulseAudio/Helpers.hpp>
#include <GhafAudioControl/Backends/PulseAudio/Volume.hpp>

#include <format>

namespace ghaf::AudioControl::Backend::PulseAudio
{

Source::Source(const pa_source_info& info, pa_context& context)
    : m_device(info, context)
{
}

bool Source::operator==(const IDevice& other) const
{
    if (const auto* otherSource = dynamic_cast<const Source*>(&other))
        return m_device == otherSource->m_device;

    return false;
}

void Source::setMuted(bool mute)
{
    deleteCheck();
    ExecutePulseFunc(pa_context_set_source_mute_by_index, &m_device.getContext(), m_device.getIndex(), mute, nullptr, nullptr);
}

void Source::setVolume(Volume volume)
{
    deleteCheck();

    pa_cvolume paChannelVolume;
    std::ignore = pa_cvolume_set(&paChannelVolume, m_device.getPulseChannelVolume().channels, ToPulseAudioVolume(volume));

    ExecutePulseFunc(pa_context_set_source_volume_by_index, &m_device.getContext(), m_device.getIndex(), &paChannelVolume, nullptr, nullptr);
}

std::string Source::toString() const
{
    return std::format("PulseSource: [ {} ]", m_device.toString());
}

std::string Source::getDescription() const
{
    return m_device.getDescription();
}

void Source::markDeleted()
{
    m_device.markDeleted();
    m_onDelete();
}

void Source::deleteCheck()
{
    if (m_device.isDeleted())
        throw std::logic_error{std::format("Using deleted device: {}", toString())};
}

uint32_t Source::getCardIndex() const
{
    return m_device.getCardIndex();
}

void Source::update(const pa_source_info& info)
{
    m_device.update(info);
    m_onUpdate();
}

void Source::update(const pa_card_info& info)
{
    m_device.update(info);
    m_onUpdate();
}

} // namespace ghaf::AudioControl::Backend::PulseAudio
