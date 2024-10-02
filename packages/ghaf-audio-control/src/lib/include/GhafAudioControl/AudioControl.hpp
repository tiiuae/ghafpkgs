/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/AppList.hpp>
#include <GhafAudioControl/IAudioControlBackend.hpp>

#include <gtkmm/menubutton.h>
#include <gtkmm/popover.h>
#include <gtkmm/separator.h>
#include <gtkmm/stacksidebar.h>

#include <glibmm/refptr.h>

namespace ghaf::AudioControl
{

class AudioControl final : public Gtk::Box
{
public:
    AudioControl(std::unique_ptr<IAudioControlBackend> backend, const std::vector<std::string>& appVmsList);
    ~AudioControl() override = default;

    AudioControl(AudioControl&) = delete;
    AudioControl(AudioControl&&) = delete;

    AudioControl& opeartor(AudioControl&) = delete;
    AudioControl& opeartor(AudioControl&&) = delete;

private:
    void init();

    void onPulseSinksChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::Sinks::IndexT extIndex, IAudioControlBackend::Sinks::PtrT sink);

    void onPulseSourcesChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::Sources::IndexT extIndex,
                               IAudioControlBackend::Sources::PtrT source);

    void onPulseSinkInputsChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::SinkInputs::IndexT extIndex,
                                  IAudioControlBackend::SinkInputs::PtrT sinkInput);

    void onPulseSourcesOutputsChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::SourceOutputs::IndexT extIndex,
                                      IAudioControlBackend::SourceOutputs::PtrT sourceOutput);
    void onPulseError(std::string_view error);

private:
    AppList m_appList;
    std::unique_ptr<IAudioControlBackend> m_audioControl;

    ConnectionContainer m_connections;
};

} // namespace ghaf::AudioControl
