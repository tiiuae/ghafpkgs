/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/widgets/AppVmWidget.hpp>

#include <GhafAudioControl/utils/Logger.hpp>

namespace ghaf::AudioControl
{

namespace
{

constexpr auto RevealerTransitionType = Gtk::RevealerTransitionType::REVEALER_TRANSITION_TYPE_SLIDE_DOWN;
constexpr auto RevealerAnimationTimeMs = 1000;

Gtk::Widget* CreateDeviceWidget(const Glib::RefPtr<Glib::Object>& deviceModelPtr)
{
    if (!deviceModelPtr)
    {
        Logger::error("AppVmWidget: deviceModelPtr is nullptr");
        return nullptr;
    }

    auto deviceModel = Glib::RefPtr<DeviceModel>::cast_dynamic<Glib::Object>(deviceModelPtr);
    if (!deviceModel)
    {
        Logger::error("AppVmWidget: deviceModel is not an AppVmModel");
        return nullptr;
    }

    return Gtk::make_managed<DeviceWidget>(deviceModel);
}

} // namespace

AppVmWidget::AppVmWidget(Glib::RefPtr<AppVmModel> model)
    : Gtk::Box(Gtk::Orientation::ORIENTATION_VERTICAL)
    , m_model(std::move(model))
    , m_appNameButton("AppVm: " + m_model->getAppNameProperty())
    , m_connection(m_appNameButton.signal_clicked().connect([this] { m_revealer.set_reveal_child(!m_revealer.get_reveal_child()); }))
{
    m_revealer.add(m_listBox);
    m_revealer.set_transition_type(RevealerTransitionType);
    m_revealer.set_transition_duration(RevealerAnimationTimeMs);
    m_revealer.set_reveal_child();

    add(m_appNameButton);
    add(m_revealer);

    m_listBox.bind_model(m_model->getDeviceModels(), &CreateDeviceWidget);
    m_listBox.set_can_focus(false);
    m_listBox.set_selection_mode(Gtk::SelectionMode::SELECTION_SINGLE);
}

void AppVmWidget::reveal(bool reveal)
{
    m_revealer.set_reveal_child(reveal);
}

} // namespace ghaf::AudioControl
