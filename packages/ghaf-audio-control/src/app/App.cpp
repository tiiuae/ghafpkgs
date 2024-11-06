/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include "App.hpp"

#include <glibmm/optioncontext.h>

#include <format>

using namespace ghaf::AudioControl;

namespace
{

constexpr auto AppId = "Ghaf Audio Control";

struct AppArgs
{
    Glib::ustring pulseServerAddress;
    Glib::ustring indicatorIconPath;
    Glib::ustring appVms;
};

std::vector<std::string> GetAppVmsList(const std::string& appVms)
{
    std::vector<std::string> result;

    std::istringstream iss(appVms);
    std::string buf;

    while (getline(iss, buf, ','))
        result.push_back(buf);

    return result;
}

} // namespace

App::AppMenu::AppMenu(App& app)
    : m_app(app)
    , m_openItem("Open/Hide Audio Control")
    , m_quitItem("Quit")
    , m_connections{m_openItem.signal_activate().connect([this]() { m_app.toggleWindow(); }), m_quitItem.signal_activate().connect([this]() { m_app.onQuit(); })}
{
    add(m_openItem);
    add(m_quitItem);

    show_all();
}

App::App(int argc, char** argv)
    : m_menu(*this)
    , m_indicator(createAppIndicator())
    , m_connections{m_dbusService.openSignal().connect(sigc::mem_fun(*this, &App::openWindow))}
{
    AppArgs appArgs;

    Glib::OptionEntry pulseServerOption;
    pulseServerOption.set_long_name("pulseaudio_server");
    pulseServerOption.set_description("PulseAudio server address");

    Glib::OptionEntry indicatorIconPathOption;
    indicatorIconPathOption.set_long_name("indicator_icon_path");
    indicatorIconPathOption.set_description("Tray's icon indicator path");

    Glib::OptionEntry appVmsOption;
    appVmsOption.set_long_name("app_vms");
    appVmsOption.set_description("AppVMs list");

    Glib::OptionGroup options("Main", "Main");
    options.add_entry(pulseServerOption, appArgs.pulseServerAddress);
    options.add_entry(indicatorIconPathOption, appArgs.indicatorIconPath);
    options.add_entry(appVmsOption, appArgs.appVms);

    Glib::OptionContext context("Application Options");
    context.set_main_group(options);

    if (!context.parse(argc, argv))
    {
        Logger::info(context.get_help().c_str());
        throw std::runtime_error{"Couldn't parse the command line arguments"};
    }

    app_indicator_set_icon(m_indicator.get(), appArgs.indicatorIconPath.c_str());

    m_audioControl = std::make_unique<AudioControl>(std::make_unique<Backend::PulseAudio::AudioControlBackend>(appArgs.pulseServerAddress),
                                                    GetAppVmsList(appArgs.appVms));
}

int App::start(int argc, char** argv)
{
    Logger::debug(__PRETTY_FUNCTION__);
    return run(argc, argv);
}

bool App::onWindowDelete([[maybe_unused]] GdkEventAny* event)
{
    Logger::debug(__PRETTY_FUNCTION__);

    m_window->hide();
    return true;
}

void App::openWindow()
{
    m_window->show_all();
    m_window->present();
}

void App::toggleWindow()
{
    auto& window = *m_window;

    ghaf::AudioControl::Logger::debug(std::format("Indicator has been activated. window.is_visible: {}", window.is_visible()));

    if (window.is_visible())
        window.hide();
    else
        openWindow();
}

void App::onQuit()
{
    release();
    quit();
}

void App::on_activate()
{
    Logger::debug(__PRETTY_FUNCTION__);

    m_window = std::make_unique<Gtk::ApplicationWindow>();
    m_window->set_title(AppId);
    m_window->add(*m_audioControl);

    m_window->signal_delete_event().connect(sigc::mem_fun(*this, &App::onWindowDelete));

    hold();
    add_window(*m_window);

    m_window->show_all();
}

RaiiWrap<AppIndicator*> App::createAppIndicator()
{
    const auto contructor = [this](AppIndicator*& indicator)
    {
        indicator = app_indicator_new(AppId, "", APP_INDICATOR_CATEGORY_APPLICATION_STATUS);
        app_indicator_set_status(indicator, APP_INDICATOR_STATUS_ACTIVE);
        app_indicator_set_label(indicator, AppId, AppId);
        app_indicator_set_title(indicator, AppId);
        app_indicator_set_menu(indicator, GTK_MENU(m_menu.gobj()));
    };

    return {contructor, {}};
}
