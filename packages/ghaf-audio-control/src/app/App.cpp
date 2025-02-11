/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include "App.hpp"

#include <gtkmm/icontheme.h>

#include <glibmm/main.h>
#include <glibmm/optioncontext.h>

#include <ranges>

using namespace ghaf::AudioControl;

namespace
{

constexpr auto AppId = "Ghaf Audio Control";

struct AppArgs
{
    Glib::ustring pulseServerAddress;
    Glib::ustring indicatorIconName;
    Glib::ustring appVms;
    Glib::ustring isDeamonMode;
    Glib::ustring allowMultipleStreamsPerVm;
};

std::vector<std::string> GetAppVmsList(const std::string& appVms)
{
    std::vector<std::string> result;

    for (auto part : appVms | std::views::split(','))
        result.emplace_back(part.begin(), part.end());

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
    : Gtk::Application("org.ghaf.AudioControl", Gio::APPLICATION_HANDLES_COMMAND_LINE)
    , m_menu(*this)
    , m_connections{m_dbusService.openSignal().connect(sigc::mem_fun(*this, &App::openWindow)),
                    m_dbusService.toggleSignal().connect(sigc::mem_fun(*this, &App::toggleWindow))}
{
    AppArgs appArgs;

    Glib::OptionEntry pulseServerOption;
    pulseServerOption.set_long_name("pulseaudio_server");
    pulseServerOption.set_description("PulseAudio server address");

    Glib::OptionEntry indicatorIconNameOption;
    indicatorIconNameOption.set_long_name("indicator_icon_name");
    indicatorIconNameOption.set_description("Tray's icon indicator name");

    Glib::OptionEntry appVmsOption;
    appVmsOption.set_long_name("app_vms");
    appVmsOption.set_description("AppVMs list");

    Glib::OptionEntry deamonModeOption;
    deamonModeOption.set_long_name("deamon_mode");
    deamonModeOption.set_description("Deamon mode");

    Glib::OptionEntry allowMultipleStreamsPerVmOption;
    allowMultipleStreamsPerVmOption.set_long_name("allow_multiple_streams_per_vm");
    allowMultipleStreamsPerVmOption.set_description("Allow Multiple Streams Per Vm");

    Glib::OptionGroup options("Main", "Main");
    options.add_entry(pulseServerOption, appArgs.pulseServerAddress);
    options.add_entry(indicatorIconNameOption, appArgs.indicatorIconName);
    options.add_entry(appVmsOption, appArgs.appVms);
    options.add_entry(deamonModeOption, appArgs.isDeamonMode);
    options.add_entry(allowMultipleStreamsPerVmOption, appArgs.allowMultipleStreamsPerVm);

    Glib::OptionContext context("Application Options");
    context.set_main_group(options);

    if (!context.parse(argc, argv))
    {
        Logger::info(context.get_help().c_str());
        throw std::runtime_error{"Couldn't parse the command line arguments"};
    }

    Logger::info("Parsed the option: '{}' = '{}'", pulseServerOption.get_long_name().c_str(), appArgs.pulseServerAddress.c_str());
    Logger::info("Parsed the option: '{}' = '{}'", indicatorIconNameOption.get_long_name().c_str(), appArgs.indicatorIconName.c_str());
    Logger::info("Parsed the option: '{}' = '{}'", appVmsOption.get_long_name().c_str(), appArgs.appVms.c_str());
    Logger::info("Parsed the option: '{}' = '{}'", deamonModeOption.get_long_name().c_str(), appArgs.isDeamonMode.c_str());
    Logger::info("Parsed the option: '{}' = '{}'", allowMultipleStreamsPerVmOption.get_long_name().c_str(), appArgs.allowMultipleStreamsPerVm.c_str());

    m_connections += signal_command_line().connect(
        [this]([[maybe_unused]] const Glib::RefPtr<Gio::ApplicationCommandLine>& args)
        {
            hold();
            return 0;
        },
        false);

    m_audioControlBackend = std::make_shared<Backend::PulseAudio::AudioControlBackend>(appArgs.pulseServerAddress);

    m_indicator = createAppIndicator();
    app_indicator_set_icon(m_indicator->get(), appArgs.indicatorIconName.c_str());

    // if (!appArgs.indicatorIconName.empty())
    // {
    //     Glib::signal_idle().connect(
    //         [this, iconName = appArgs.indicatorIconName]() -> bool
    //         {
    //             Logger::info("Setting up an indicator icon: {}", iconName.c_str());
    //             m_dbusService.registerSystemTrayIcon(iconName);

    //             return false;
    //         });
    // }
    // else
    // {
    //     Logger::info("No indicator icon was specified");
    // }

    const std::weak_ptr weakBackend = m_audioControlBackend;

    m_connections += m_dbusService.subscribeToDeviceUpdatedSignal().connect(
        [this, weakBackend]()
        {
            if (auto strong = weakBackend.lock())
            {
                for (const auto& device : strong->getAllDevices())
                    sendDeviceUpdateToDbus({.eventType = DBusService::DeviceEventType::Add, .index = device->getIndex(), .type = device->getType(), .ptr = device});
            }
        });

    m_connections += m_dbusService.setDeviceVolumeSignal().connect(
        [this, weakBackend](auto id, auto type, auto volume)
        {
            if (type == IAudioControlBackend::IDevice::Type::Meta)
                m_metaDeviceManager.setDeviceVolume(id, volume);
            else if (auto backend = weakBackend.lock())
                backend->setDeviceVolume(id, type, volume);
            else
                Logger::error("m_dbusService.setDeviceVolumeSignal().connect: backend doesn't exist anymore");
        });

    m_connections += m_dbusService.setDeviceMuteSignal().connect(
        [this, weakBackend](auto id, auto type, auto mute)
        {
            if (type == IAudioControlBackend::IDevice::Type::Meta)
                m_metaDeviceManager.setDeviceMute(id, mute);
            if (auto backend = weakBackend.lock())
                backend->setDeviceMute(id, type, mute);
            else
                Logger::error("m_dbusService.setDeviceMuteSignal().connect: backend doesn't exist anymore");
        });

    m_connections += m_dbusService.makeDeviceDefaultSignal().connect(
        [weakBackend](auto id, auto type)
        {
            if (auto backend = weakBackend.lock())
                backend->makeDeviceDefault(id, type);
            else
                Logger::error("m_dbusService.setDeviceMuteSignal().connect: backend doesn't exist anymore");
        });

    m_audioControl = std::make_unique<AudioControl>(GetAppVmsList(appArgs.appVms), appArgs.allowMultipleStreamsPerVm == "true");

    const auto onDevice = [this](const IAudioControlBackend::OnSignalMapChangeSignalInfo& info)
    {
        if (info.type == IAudioControlBackend::IDevice::Type::SinkInput)
            m_metaDeviceManager.sendDeviceInfoUpdate(info);

        m_audioControl->sendDeviceInfoUpdate(info);
        sendDeviceUpdateToDbus(info);
    };

    const auto onError = [this](std::string_view error)
    {
        m_audioControl->sendError(error);
    };

    m_connections += m_audioControlBackend->onSinksChanged().connect(onDevice);
    m_connections += m_audioControlBackend->onSourcesChanged().connect(onDevice);
    m_connections += m_audioControlBackend->onSinkInputsChanged().connect(onDevice);
    // m_connections += m_audioControlBackend->onSourceOutputsChanged().connect(onDevice); // We don't it now
    m_connections += m_audioControlBackend->onError().connect(onError);
    m_connections += m_metaDeviceManager.onDeviceUpdateSignal().connect(onDevice);

    m_audioControlBackend->start();
}

int App::start()
{
    Logger::debug(__PRETTY_FUNCTION__);
    return run();
}

bool App::onWindowDelete([[maybe_unused]] GdkEventAny* event)
{
    Logger::debug(__PRETTY_FUNCTION__);

    m_window->hide();
    return true;
}

void App::openWindow()
{
    Logger::debug(__PRETTY_FUNCTION__);

    if (m_window == nullptr)
        on_activate();

    m_window->show();
    m_window->present();
}

void App::toggleWindow()
{
    Logger::debug(__PRETTY_FUNCTION__);

    if (m_window == nullptr)
    {
        on_activate();
        openWindow();

        return;
    }

    auto& window = *m_window;

    Logger::debug("Indicator has been activated. window.is_visible: {}", window.is_visible());

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

    m_window->show();
    m_audioControl->show();
}

void App::sendDeviceUpdateToDbus(const ghaf::AudioControl::IAudioControlBackend::OnSignalMapChangeSignalInfo& info)
{
    if (info.ptr)
    {
        if (auto* defaultable = dynamic_cast<IAudioControlBackend::IDefaultable*>(info.ptr.get()))
            m_dbusService.sendDeviceInfo(info.index,
                                         info.type,
                                         info.ptr->getDescription(),
                                         info.ptr->getVolume(),
                                         info.ptr->isMuted(),
                                         defaultable->isDefault(),
                                         info.eventType);
        else
            m_dbusService.sendDeviceInfo(info.index, info.type, info.ptr->getName(), info.ptr->getVolume(), info.ptr->isMuted(), false, info.eventType);
    }
    else
        m_dbusService.sendDeviceInfo(info.index, info.type, "Deleted", Volume::fromPercents(0U), false, false, IAudioControlBackend::EventType::Delete);
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
