/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/Sink.hpp>

#include <GhafAudioControl/Backends/PulseAudio/Helpers.hpp>
#include <GhafAudioControl/Backends/PulseAudio/Volume.hpp>

#include <format>

namespace ghaf::AudioControl::Backend::PulseAudio
{

Sink::Sink(const pa_sink_info& info, bool isDefault, pa_context& context)
    : m_device(info, isDefault, context)
{
}

bool Sink::operator==(const IDevice& other) const
{
    if (const Sink* otherSink = dynamic_cast<const Sink*>(&other))
        return m_device == otherSink->m_device;

    return false;
}

void Sink::setMuted(bool mute)
{
    deleteCheck();
    ExecutePulseFunc(pa_context_set_sink_mute_by_index, &m_device.getContext(), m_device.getIndex(), mute, nullptr, nullptr);
}

void Sink::setVolume(Volume volume)
{
    deleteCheck();

    pa_cvolume paChannelVolume;
    std::ignore = pa_cvolume_set(&paChannelVolume, m_device.getPulseChannelVolume().channels, ToPulseAudioVolume(volume));

    ExecutePulseFunc(pa_context_set_sink_volume_by_index, &m_device.getContext(), m_device.getIndex(), &paChannelVolume, nullptr, nullptr);
}

std::string Sink::toString() const
{
    return std::format("PulseSink: [ {} ]", m_device.toString());
}

void Sink::markDeleted()
{
    m_device.markDeleted();
    m_onDelete();
}

void Sink::setDefault(bool value)
{
    deleteCheck();

    if (m_device.isDefault() == value)
        return;

    ExecutePulseFunc(pa_context_set_default_sink, &m_device.getContext(), m_device.getName().c_str(), nullptr, nullptr);
}

void Sink::deleteCheck()
{
    if (m_device.isDeleted())
        throw std::logic_error{std::format("Using deleted device: {}", toString())};
}

void Sink::updateDefault(bool value)
{
    if (m_device.isDefault() == value)
        return;

    m_device.setDefault(value);
    m_onUpdate();
}

void Sink::update(const pa_sink_info& info)
{
    m_device.update(info);
    m_onUpdate();
}

void Sink::update(const pa_card_info& info)
{
    m_device.update(info);
    m_onUpdate();
}
} // namespace ghaf::AudioControl::Backend::PulseAudio
