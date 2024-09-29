/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/models/AppVmModel.hpp>
#include <GhafAudioControl/widgets/DeviceWidget.hpp>

#include <gtkmm/box.h>
#include <gtkmm/button.h>
#include <gtkmm/listbox.h>
#include <gtkmm/revealer.h>
#include <gtkmm/stack.h>

namespace ghaf::AudioControl
{

class AppVmWidget final : public Gtk::Box
{
public:
    explicit AppVmWidget(Glib::RefPtr<AppVmModel> model);

    void reveal(bool reveal = true);

private:
    Glib::RefPtr<AppVmModel> m_model;

    Gtk::ListBox m_listBox;
    Gtk::Button m_appNameButton;
    Gtk::Revealer m_revealer;

    sigc::connection m_connection;
};

} // namespace ghaf::AudioControl
