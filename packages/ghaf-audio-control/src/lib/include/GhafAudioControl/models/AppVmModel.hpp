/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/models/DeviceModel.hpp>

#include <glibmm/object.h>
#include <glibmm/property.h>

#include <giomm/liststore.h>

namespace ghaf::AudioControl
{

class AppVmModel final : public Glib::Object
{
public:
    using AppIdType = std::string;

private:
    explicit AppVmModel(AppIdType id);

public:
    static Glib::RefPtr<AppVmModel> create(AppIdType id);

    static int compare(const Glib::RefPtr<const AppVmModel>& a, const Glib::RefPtr<const AppVmModel>& b);

    void addSinkInput(IAudioControlBackend::ISinkInput::Ptr sinkInput);
    void deleteSinkInput(const IAudioControlBackend::ISinkInput::Ptr& sinkInput);

    [[nodiscard]] Glib::RefPtr<Gio::ListStore<DeviceModel>> getDeviceModels() noexcept
    {
        return m_devices;
    }

    [[nodiscard]] auto getIsEnabledProperty() const
    {
        return m_isEnabled.get_proxy();
    }

    [[nodiscard]] auto getAppNameProperty() const
    {
        return m_appName.get_proxy();
    }

    [[nodiscard]] auto getIconUrlProperty() const
    {
        return m_iconUrl.get_proxy();
    }

private:
    Glib::RefPtr<Gio::ListStore<DeviceModel>> m_devices;
    std::map<IAudioControlBackend::IDevice::IntexT, size_t> m_deviceIndex;

    Glib::Property<Glib::ustring> m_appName;
    Glib::Property_ReadOnly<Glib::ustring> m_iconUrl{*this, "m_iconUrl", "/usr/share/pixmaps/ubuntu-logo.svg"};

    Glib::Property<bool> m_isEnabled{*this, "m_isEnabled", false};

    std::map<IAudioControlBackend::IDevice::IntexT, sigc::connection> m_deviceConnections;
};

} // namespace ghaf::AudioControl
