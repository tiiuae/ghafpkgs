/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/widgets/AppList.hpp>
#include <GhafAudioControl/widgets/DeviceListWidget.hpp>

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
    AudioControl(std::shared_ptr<IAudioControlBackend> backend, const std::vector<std::string>& appVmsList);
    ~AudioControl() override = default;

    AudioControl(AudioControl&) = delete;
    AudioControl(AudioControl&&) = delete;

    AudioControl& opeartor(AudioControl&) = delete;
    AudioControl& opeartor(AudioControl&&) = delete;

private:
    void init();

    void onPulseSinksChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info);
    void onPulseSourcesChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info);
    void onPulseSinkInputsChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info);
    void onPulseSourcesOutputsChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info);

    void onPulseError(std::string_view error);

private:
    std::shared_ptr<IAudioControlBackend> m_audioControl;

    AppList m_appList;

    Glib::RefPtr<DeviceListModel> m_sinksModel;
    DeviceListWidget m_sinks;

    Glib::RefPtr<DeviceListModel> m_sourcesModel;
    DeviceListWidget m_sources;

    ConnectionContainer m_connections;
};

} // namespace ghaf::AudioControl
