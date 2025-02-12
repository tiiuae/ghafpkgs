/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp>
#include <GhafAudioControl/utils/Debug.hpp>
#include <GhafAudioControl/utils/Logger.hpp>
#include <GhafAudioControl/widgets/AudioControl.hpp>

#include <GhafAudioControl/AppVmMetaGroup.hpp>

#include "DBusService.hpp"

#include <gtkmm/application.h>
#include <gtkmm/applicationwindow.h>

#include <libayatana-appindicator/app-indicator.h>

class App : public Gtk::Application
{
private:
    class AppMenu : public Gtk::Menu
    {
    public:
        explicit AppMenu(App& app);
        ~AppMenu() override = default;

    private:
        App& m_app;

        Gtk::MenuItem m_openItem;
        Gtk::MenuItem m_quitItem;

        ghaf::AudioControl::ConnectionContainer m_connections;
    };

public:
    App(int argc, char** argv);

    int start();

    [[nodiscard]] ghaf::AudioControl::RaiiWrap<AppIndicator*> createAppIndicator();

    void openWindow();
    void toggleWindow();

private:
    bool onWindowDelete([[maybe_unused]] GdkEventAny* event);
    void onQuit();

    void on_activate() override;

    void sendDeviceUpdateToDbus(const ghaf::AudioControl::IAudioControlBackend::OnSignalMapChangeSignalInfo& info, std::optional<std::string> destination);

private:
    std::shared_ptr<ghaf::AudioControl::Backend::PulseAudio::AudioControlBackend> m_audioControlBackend;

    ghaf::AudioControl::MetaDeviceManager m_metaDeviceManager;

    DBusService m_dbusService;

    std::unique_ptr<ghaf::AudioControl::AudioControl> m_audioControl;
    std::unique_ptr<Gtk::ApplicationWindow> m_window;

    AppMenu m_menu;
    std::optional<ghaf::AudioControl::RaiiWrap<AppIndicator*>> m_indicator;

    ghaf::AudioControl::ConnectionContainer m_connections;
};
