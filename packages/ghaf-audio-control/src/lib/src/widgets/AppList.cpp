/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/SinkInput.hpp>
#include <GhafAudioControl/utils/Check.hpp>
#include <GhafAudioControl/utils/Debug.hpp>
#include <GhafAudioControl/utils/Logger.hpp>
#include <GhafAudioControl/widgets/AppList.hpp>
#include <GhafAudioControl/widgets/DeviceListWidget.hpp>

#include <gtkmm/adjustment.h>
#include <gtkmm/button.h>
#include <gtkmm/enums.h>
#include <gtkmm/image.h>
#include <gtkmm/label.h>
#include <gtkmm/revealer.h>
#include <gtkmm/scale.h>
#include <gtkmm/switch.h>

namespace ghaf::AudioControl
{

namespace
{

constexpr auto AppVmPrefix = "AppVM";

std::optional<size_t> GetIndexByAppId(const Glib::RefPtr<Gio::ListStore<DeviceListModel>>& model, const std::string& id) noexcept
{
    for (size_t index = 0; index < model->get_n_items(); ++index)
        if (id == model->get_item(index)->getAppNameProperty().get_value())
            return index;

    return std::nullopt;
}

std::string GetAppNameFromSinkInput(const IAudioControlBackend::ISinkInput::Ptr& device)
{
    if (auto sinkInput = std::dynamic_pointer_cast<Backend::PulseAudio::SinkInput>(device))
    {
        if (const auto appVmName = sinkInput->getAppVmName())
            return *appVmName;
    }

    return "Other";
}

Gtk::Widget* CreateWidgetsForApp(const Glib::RefPtr<Glib::Object>& appVmModelPtr)
{
    if (!appVmModelPtr)
    {
        Logger::error("AppList: appVmModelPtr is nullptr");
        return nullptr;
    }

    auto appVmModel = Glib::RefPtr<DeviceListModel>::cast_dynamic<Glib::Object>(appVmModelPtr);
    if (!appVmModel)
    {
        Logger::error("AppList: appVmModel is not an AppVmModel");
        return nullptr;
    }

    return Gtk::make_managed<DeviceListWidget>(appVmModel);
}

} // namespace

AppList::AppList()
    : Gtk::Box(Gtk::ORIENTATION_HORIZONTAL)
    , m_appsModel(Gio::ListStore<DeviceListModel>::create())
{
    m_listBox.bind_model(m_appsModel, &CreateWidgetsForApp);
    m_listBox.set_can_focus(false);
    m_listBox.set_selection_mode(Gtk::SelectionMode::SELECTION_SINGLE);

    pack_start(m_listBox, Gtk::PACK_EXPAND_WIDGET);
}

void AppList::addVm(std::string appVmName)
{
    if (const auto index = GetIndexByAppId(m_appsModel, appVmName))
        return;

    m_appsModel->append(DeviceListModel::create(std::move(appVmName), AppVmPrefix));
}

void AppList::addDevice(IAudioControlBackend::ISinkInput::Ptr device)
{
    Check(device != nullptr, "device is nullptr");

    const std::string appName = GetAppNameFromSinkInput(device);

    if (const auto index = GetIndexByAppId(m_appsModel, appName))
        m_appsModel->get_item(*index)->addDevice(std::move(device));
    else
    {
        Logger::info("AppList::addDevice: add new app with name: {}", appName);

        auto appVmModel = DeviceListModel::create(appName, AppVmPrefix);
        m_appsModel->append(appVmModel);

        appVmModel->addDevice(std::move(device));
    }

    show_all_children(true);
}

void AppList::removeAllApps()
{
    m_appsModel->remove_all();
}

} // namespace ghaf::AudioControl
