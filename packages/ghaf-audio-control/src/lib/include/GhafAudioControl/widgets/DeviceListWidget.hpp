/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/models/DeviceListModel.hpp>

#include <gtkmm/box.h>
#include <gtkmm/button.h>
#include <gtkmm/cssprovider.h>
#include <gtkmm/label.h>
#include <gtkmm/listbox.h>
#include <gtkmm/revealer.h>
#include <gtkmm/separator.h>
#include <gtkmm/stack.h>

#include <glibmm/binding.h>

namespace ghaf::AudioControl
{

class DeviceListWidget : public Gtk::Box
{
public:
    explicit DeviceListWidget(Glib::RefPtr<DeviceListModel> devicesModel);
    ~DeviceListWidget() override = default;

    void reveal(bool reveal = true);

private:
    void onDeviceChange(guint position, guint removed, guint added);

    std::string getName() const;

private:
    Glib::RefPtr<DeviceListModel> m_model;

    Gtk::Box m_revealerBox;
    Gtk::ListBox m_listBox;

    Gtk::Label m_emptyListLabel;
    Gtk::Button m_appNameButton;
    Gtk::Revealer m_revealer;

    ConnectionContainer m_connections;
};

} // namespace ghaf::AudioControl
