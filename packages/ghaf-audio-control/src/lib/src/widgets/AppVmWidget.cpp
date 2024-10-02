/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/widgets/AppVmWidget.hpp>

#include <GhafAudioControl/utils/Debug.hpp>
#include <GhafAudioControl/utils/Logger.hpp>

#include <format>

#include <glibmm/main.h>

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
    , m_revealerBox(Gtk::Orientation::ORIENTATION_VERTICAL)
    , m_emptyListLabel("No streams from the AppVM")
    , m_appNameButton("AppVm: " + m_model->getAppNameProperty())
    , m_connections{m_appNameButton.signal_clicked().connect([this] { m_revealer.set_reveal_child(!m_revealer.get_reveal_child()); }),
                    m_model->getDeviceModels()->signal_items_changed().connect(sigc::mem_fun(*this, &AppVmWidget::onDeviceChange))}
{
    m_emptyListLabel.set_name("EmptyListName");
    m_emptyListLabel.set_halign(Gtk::Align::ALIGN_FILL);

    m_revealerBox.pack_start(m_emptyListLabel);
    m_revealerBox.pack_end(m_listBox);

    m_revealer.add(m_revealerBox);
    m_revealer.set_transition_type(RevealerTransitionType);
    m_revealer.set_transition_duration(RevealerAnimationTimeMs);
    m_revealer.set_reveal_child();

    m_appNameButton.set_name("AppVmNameButton");
    m_appNameButton.set_halign(Gtk::Align::ALIGN_START);

    pack_start(m_appNameButton);
    pack_start(m_revealer);

    m_listBox.bind_model(m_model->getDeviceModels(), &CreateDeviceWidget);
    m_listBox.set_can_focus(false);
    m_listBox.set_selection_mode(Gtk::SelectionMode::SELECTION_SINGLE);
}

void AppVmWidget::onDeviceChange([[maybe_unused]] guint position, [[maybe_unused]] guint removed, [[maybe_unused]] guint added)
{
    Glib::signal_idle().connect_once([this]() { m_emptyListLabel.set_visible(m_model->getDeviceModels()->get_n_items() == 0); }, Glib::PRIORITY_DEFAULT);
}

void AppVmWidget::reveal(bool reveal)
{
    CheckUiThread();
    m_revealer.set_reveal_child(reveal);
}

} // namespace ghaf::AudioControl
