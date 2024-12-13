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

class DeviceListModel final : public Glib::Object
{
private:
    explicit DeviceListModel(std::string name, std::string namePrefix = "");

public:
    static Glib::RefPtr<DeviceListModel> create(std::string name, std::string namePrefix = "");

    static int compare(const Glib::RefPtr<const DeviceListModel>& a, const Glib::RefPtr<const DeviceListModel>& b);

    void addDevice(IAudioControlBackend::ISink::Ptr device);

    [[nodiscard]] Glib::RefPtr<Gio::ListStore<DeviceModel>> getDeviceModels() noexcept
    {
        return m_devices;
    }

    [[nodiscard]] auto getAppNameProperty() const
    {
        return m_name.get_proxy();
    }

    [[nodiscard]] auto getNamePrefix() const
    {
        return m_namePrefix;
    }

private:
    Glib::Property<Glib::ustring> m_name;
    std::string m_namePrefix;

    Glib::RefPtr<Gio::ListStore<DeviceModel>> m_devices;
    std::map<IAudioControlBackend::IDevice::IntexT, sigc::connection> m_deviceConnections;
};

} // namespace ghaf::AudioControl
