/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/AudioControl.hpp>
#include <GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp>
#include <GhafAudioControl/utils/Debug.hpp>
#include <GhafAudioControl/utils/Logger.hpp>

#include <glibmm/main.h>
#include <glibmm/optioncontext.h>
#include <gtkmm/applicationwindow.h>

#include <format>

using namespace ghaf::AudioControl;

namespace
{

std::vector<std::string> GetAppVmsList(const std::string& appVms)
{
    std::vector<std::string> result;

    std::istringstream iss(appVms);
    std::string buf;

    while (getline(iss, buf, ','))
        result.push_back(buf);

    return result;
}

int GtkClient(const std::string& pulseAudioServerAddress, const std::string& appVms)
{
    auto app = Gtk::Application::create();
    Gtk::ApplicationWindow window;

    AudioControl audioControl{std::make_unique<Backend::PulseAudio::AudioControlBackend>(pulseAudioServerAddress), GetAppVmsList(appVms)};

    window.add(audioControl);
    window.show_all();

    return app->run(window);
}

} // namespace

int main(int argc, char** argv)
{
    pthread_setname_np(pthread_self(), "main");

    Glib::ustring pulseServerAddress;
    Glib::OptionEntry pulseServerOption;
    pulseServerOption.set_long_name("pulseaudio_server");
    pulseServerOption.set_description("PulseAudio server address");

    Glib::ustring appVms;
    Glib::OptionEntry appVmsOption;
    appVmsOption.set_long_name("app_vms");
    appVmsOption.set_description("AppVMs list");

    Glib::OptionGroup options("Main", "Main");
    options.add_entry(pulseServerOption, pulseServerAddress);
    options.add_entry(appVmsOption, appVms);

    Glib::OptionContext context("Application Options");
    context.set_main_group(options);

    try
    {
        if (!context.parse(argc, argv))
            throw std::runtime_error{"Couldn't parse command line arguments"};
    }
    catch (const Glib::Error& ex)
    {
        Logger::error(std::format("Error: {}", ex.what().c_str()));
        Logger::info(context.get_help().c_str());

        return 1;
    }

    return GtkClient(pulseServerAddress, appVms);
}
