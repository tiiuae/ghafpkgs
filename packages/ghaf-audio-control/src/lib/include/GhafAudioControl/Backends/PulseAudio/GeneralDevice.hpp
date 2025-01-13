/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <GhafAudioControl/Volume.hpp>

#include <sigc++/signal.h>

#include <pulse/context.h>
#include <pulse/introspect.h>
#include <pulse/volume.h>

#include <mutex>
#include <string>

namespace ghaf::AudioControl::Backend::PulseAudio
{

class GeneralDeviceImpl final
{
public:
    GeneralDeviceImpl(const pa_sink_info& info, bool isDefault, pa_context& context);
    GeneralDeviceImpl(const pa_source_info& info, bool isDefault, pa_context& context);
    GeneralDeviceImpl(const pa_sink_input_info& info, pa_context& context);
    GeneralDeviceImpl(const pa_source_output_info& info, pa_context& context);

    bool operator==(const GeneralDeviceImpl& other) const
    {
        return m_index == other.m_index;
    }

    [[nodiscard]] uint32_t getIndex() const noexcept
    {
        return m_index;
    }

    [[nodiscard]] uint32_t getCardIndex() const noexcept;

    virtual void setDefault(bool value);
    [[nodiscard]] bool isDefault() const noexcept;

    [[nodiscard]] bool isDeleted() const noexcept;
    [[nodiscard]] bool isEnabled() const noexcept;
    [[nodiscard]] bool isMuted() const;

    [[nodiscard]] Volume getVolume() const;

    [[nodiscard]] pa_volume_t getPulseVolume() const;
    [[nodiscard]] pa_cvolume getPulseChannelVolume() const noexcept;

    [[nodiscard]] std::optional<std::string> getAppVmName() const noexcept;

    [[nodiscard]] std::string getName() const;
    [[nodiscard]] std::string getDescription() const;

    [[nodiscard]] pa_context& getContext() const noexcept
    {
        return m_context;
    }

    void update(const pa_sink_info& info);
    void update(const pa_source_info& info);
    void update(const pa_sink_input_info& info);
    void update(const pa_source_output_info& info);

    void update(const pa_card_info& info);

    void markDeleted();

    [[nodiscard]] std::string toString() const;

private:
    void deleteCheck();

private:
    const uint32_t m_index;
    uint32_t m_cardIndex;

    bool m_isDefault = false;
    bool m_isDeleted = false;
    bool m_isEnabled = false;

    std::optional<std::string> m_appVmName;

    std::string m_name;
    std::string m_description;

    pa_context& m_context;

    pa_channel_map m_channel_map;
    pa_cvolume m_volume;
    bool m_isMuted;

    mutable std::mutex m_mutex;
};

} // namespace ghaf::AudioControl::Backend::PulseAudio
