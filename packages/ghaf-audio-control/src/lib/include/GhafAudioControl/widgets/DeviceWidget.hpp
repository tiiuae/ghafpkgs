/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/models/DeviceModel.hpp>
#include <GhafAudioControl/utils/ConnectionContainer.hpp>
#include <GhafAudioControl/utils/ScopeExit.hpp>

#include <gtkmm/box.h>
#include <gtkmm/checkbutton.h>
#include <gtkmm/label.h>
#include <gtkmm/scale.h>
#include <gtkmm/switch.h>

#include <glibmm/binding.h>
#include <glibmm/object.h>
#include <glibmm/property.h>

namespace ghaf::AudioControl
{

class DeviceWidget : public Gtk::Box
{
public:
    explicit DeviceWidget(DeviceModel::Ptr model);

private:
    DeviceModel::Ptr m_model;

    Gtk::CheckButton* m_defaultButton;
    Gtk::Label* m_nameLabel;
    Gtk::Switch* m_switch;
    Gtk::Scale* m_scale;

    std::vector<Glib::RefPtr<Glib::Binding>> m_bindings;
};

} // namespace ghaf::AudioControl
