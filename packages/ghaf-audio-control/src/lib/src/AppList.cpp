/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/AppList.hpp>
#include <GhafAudioControl/widgets/AppVmWidget.hpp>

#include <GhafAudioControl/utils/Logger.hpp>

#include <gtkmm/adjustment.h>
#include <gtkmm/button.h>
#include <gtkmm/enums.h>
#include <gtkmm/image.h>
#include <gtkmm/label.h>
#include <gtkmm/revealer.h>
#include <gtkmm/scale.h>
#include <gtkmm/switch.h>

#include <format>

namespace ghaf::AudioControl
{

namespace
{

std::optional<size_t> GetIndexByAppId(const Glib::RefPtr<Gio::ListStore<AppVmModel>>& model, const AppVmModel::AppIdType& id) noexcept
{
    for (size_t index = 0; index < model->get_n_items(); ++index)
        if (id == model->get_item(index)->getAppNameProperty().get_value())
            return index;

    return std::nullopt;
}

std::string GetAppNameFromSinkInput(const IAudioControlBackend::ISinkInput::Ptr& device)
{
    return "Other";
}

Gtk::Widget* CreateWidgetsForApp(const Glib::RefPtr<Glib::Object>& appVmModelPtr)
{
    if (!appVmModelPtr)
    {
        Logger::error("AppList: appVmModelPtr is nullptr");
        return nullptr;
    }

    auto appVmModel = Glib::RefPtr<AppVmModel>::cast_dynamic<Glib::Object>(appVmModelPtr);
    if (!appVmModel)
    {
        Logger::error("AppList: appVmModel is not an AppVmModel");
        return nullptr;
    }

    return Gtk::make_managed<AppVmWidget>(appVmModel);
}

} // namespace

AppList::AppList()
    : Gtk::Box(Gtk::ORIENTATION_HORIZONTAL)
    , m_appsModel(Gio::ListStore<AppVmModel>::create())
{
    m_listBox.bind_model(m_appsModel, &CreateWidgetsForApp);
    m_listBox.set_can_focus(false);
    m_listBox.set_selection_mode(Gtk::SelectionMode::SELECTION_SINGLE);

    pack_start(m_listBox, Gtk::PACK_EXPAND_WIDGET);
}

void AppList::addDevice(IAudioControlBackend::ISinkInput::Ptr device)
{
    const std::string appName = GetAppNameFromSinkInput(device);

    if (const auto index = GetIndexByAppId(m_appsModel, appName))
        m_appsModel->get_item(*index)->addSinkInput(std::move(device));
    else
    {
        Logger::error(std::format("AppList::addDevice: add new app with name: {}", appName));

        auto appVmModel = AppVmModel::create(appName);
        appVmModel->addSinkInput(std::move(device));

        m_appsModel->append(appVmModel);
    }
}

void AppList::removeAllApps()
{
    m_appsBindings.clear();
    m_appsModel->remove_all();
}

} // namespace ghaf::AudioControl
