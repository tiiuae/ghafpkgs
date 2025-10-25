/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/MetaDevice.hpp>
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
    AudioControl(const std::vector<std::string>& appVmsList, bool allowMultipleStreamsPerVm);
    ~AudioControl() override = default;

    AudioControl(AudioControl&) = delete;
    AudioControl(AudioControl&&) = delete;

    AudioControl& opeartor(AudioControl&) = delete;
    AudioControl& opeartor(AudioControl&&) = delete;

    void sendDeviceInfoUpdate(const IAudioControlBackend::OnSignalMapChangeSignalInfo& info);
    void sendError(std::string_view error);

private:
    void init();

    void onPulseError(std::string_view error);

private:
    AppList m_appList;
    const bool m_allowMultipleStreamsPerVm;

    Glib::RefPtr<DeviceListModel> m_sinksModel;
    DeviceListWidget m_sinks;

    Glib::RefPtr<DeviceListModel> m_sourcesModel;
    DeviceListWidget m_sources;

    Glib::RefPtr<DeviceListModel> m_metaSinkInputModel;
    DeviceListWidget m_metaSinkInputs;

    ConnectionContainer m_connections;
};

} // namespace ghaf::AudioControl
