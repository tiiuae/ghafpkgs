/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp>
#include <GhafAudioControl/utils/Debug.hpp>
#include <GhafAudioControl/utils/Logger.hpp>
#include <GhafAudioControl/widgets/AudioControl.hpp>

#include <gtkmm/applicationwindow.h>
#include <gtkmm/menu.h>
#include <gtkmm/menuitem.h>
#include <gtkmm/radiomenuitem.h>
#include <gtkmm/scale.h>

#include <glibmm/main.h>
#include <glibmm/optioncontext.h>

#include <libayatana-appindicator/app-indicator.h>

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

class MyApp : public Gtk::Application
{
private:
    class AppMenu : public Gtk::Menu
    {
    public:
        AppMenu(MyApp& app)
            : m_app(app)
            , m_openItem("Open/Hide Audio Control")
            , m_quitItem("Quit")
        {
            add(m_openItem);
            add(m_quitItem);

            const auto onOpen = [this]()
            {
                auto& window = *m_app.m_window;

                Logger::debug(std::format("Indicator has been activated. window.is_visible: {}", window.is_visible()));

                if (window.is_visible())
                    window.hide();
                else
                {
                    window.show_all();
                    window.present();
                }
            };

            m_connections = {m_openItem.signal_activate().connect(onOpen), m_quitItem.signal_activate().connect([this]() { m_app.onQuit(); })};

            show_all();
        }

        ~AppMenu() override = default;

    private:
        MyApp& m_app;

        Gtk::MenuItem m_openItem;
        Gtk::MenuItem m_quitItem;

        ConnectionContainer m_connections;
    };

public:
    MyApp(int argc, char** argv)
        : m_menu(*this)
        , m_indicator(createAppIndicator())
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

    int start(int argc, char** argv)
    {
        Logger::debug(__PRETTY_FUNCTION__);
        return run(argc, argv);
    }

    [[nodiscard]] RaiiWrap<AppIndicator*> createAppIndicator()
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

protected:
    bool onWindowDelete([[maybe_unused]] GdkEventAny* event)
    {
        Logger::debug(__PRETTY_FUNCTION__);

        m_window->hide();
        return true;
    }

    void onQuit()
    {
        Logger::debug(__PRETTY_FUNCTION__);

        release();
        quit();
    }

    void on_activate() override
    {
        Logger::debug(__PRETTY_FUNCTION__);

        m_window = std::make_unique<Gtk::ApplicationWindow>();
        m_window->set_title(AppId);
        m_window->add(*m_audioControl);

        m_window->signal_delete_event().connect(sigc::mem_fun(*this, &MyApp::onWindowDelete));

        hold();
        add_window(*m_window);

        m_window->show_all();
    }

private:
    std::unique_ptr<AudioControl> m_audioControl;
    std::unique_ptr<Gtk::ApplicationWindow> m_window;

    AppMenu m_menu;
    RaiiWrap<AppIndicator*> m_indicator;
};

} // namespace

int main(int argc, char** argv)
{
    pthread_setname_np(pthread_self(), "main");

    try
    {
        MyApp app(argc, argv);
        return app.start(argc, argv);
    }
    catch (const Glib::Error& ex)
    {
        Logger::error(std::format("Error: {}", ex.what().c_str()));
        return 1;
    }
    catch (const std::exception& e)
    {
        Logger::error(std::format("Exception: {}", e.what()));
        return 1;
    }
}
