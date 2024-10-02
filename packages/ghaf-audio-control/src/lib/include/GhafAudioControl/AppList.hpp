/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/models/AppVmModel.hpp>

#include <gtkmm/box.h>
#include <gtkmm/cssprovider.h>
#include <gtkmm/listbox.h>
#include <gtkmm/separator.h>
#include <gtkmm/stack.h>

#include <glibmm/binding.h>

namespace ghaf::AudioControl
{

class AppList final : public Gtk::Box
{
public:
    AppList();

    void addVm(std::string appVmName);
    void addDevice(IAudioControlBackend::ISinkInput::Ptr device);

    void removeAllApps();

private:
    Gtk::ListBox m_listBox;

    Glib::RefPtr<Gtk::CssProvider> m_cssProvider;
    Glib::RefPtr<Gio::ListStore<AppVmModel>> m_appsModel;
    std::map<AppVmModel::AppIdType, std::vector<Glib::RefPtr<Glib::Binding>>> m_appsBindings;
};

} // namespace ghaf::AudioControl
