/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/utils/ConnectionContainer.hpp>

#include <glibmm/binding.h>
#include <glibmm/object.h>
#include <glibmm/property.h>

namespace ghaf::AudioControl
{

class DeviceModel : public Glib::Object
{
public:
    using Ptr = Glib::RefPtr<DeviceModel>;

public:
    explicit DeviceModel(IAudioControlBackend::IDevice::Ptr device);

    static Glib::RefPtr<DeviceModel> create(IAudioControlBackend::IDevice::Ptr device);

    static int compare(const Glib::RefPtr<const DeviceModel>& a, const Glib::RefPtr<const DeviceModel>& b);

    virtual void updateDevice();

    [[nodiscard]] virtual Index getIndex() const
    {
        return m_device->getIndex();
    }

    [[nodiscard]] virtual Glib::PropertyProxy_ReadOnly<bool> getIsEnabledProperty() const
    {
        return m_isEnabled.get_proxy();
    }

    [[nodiscard]] virtual Glib::PropertyProxy<bool> getIsDefaultProperty()
    {
        return m_isDefault.get_proxy();
    }

    [[nodiscard]] virtual Glib::PropertyProxy_ReadOnly<bool> getHasDeviceProperty() const
    {
        return m_hasDevice.get_proxy();
    }

    [[nodiscard]] auto getNameProperty() const
    {
        return m_name.get_proxy();
    }

    [[nodiscard]] auto getIconUrlProperty() const
    {
        return m_iconUrl.get_proxy();
    }

    [[nodiscard]] virtual Glib::PropertyProxy<bool> getSoundEnabledProperty()
    {
        return m_isSoundEnabled.get_proxy();
    }

    [[nodiscard]] virtual Glib::PropertyProxy<double> getSoundVolumeProperty()
    {
        return m_soundVolume.get_proxy();
    }

private:
    void onDefaultChange();
    void onSoundEnabledChange();
    void onSoundVolumeChange();

private:
    IAudioControlBackend::IDevice::Ptr m_device;

    Glib::Property<bool> m_isEnabled;
    Glib::Property<bool> m_isDefault{*this, "m_isDefault", false};
    Glib::Property<bool> m_hasDevice{*this, "m_hasDevice", false};

    Glib::Property<Glib::ustring> m_name{*this, "m_name", "Undefined"};
    Glib::Property_ReadOnly<Glib::ustring> m_iconUrl{*this, "m_iconUrl", "/usr/share/pixmaps/ubuntu-logo.svg"};

    Glib::Property<bool> m_isSoundEnabled{*this, "m_isSoundEnabled", false};
    Glib::Property<double> m_soundVolume{*this, "m_soundVolume", 0};

    ConnectionContainer m_connections;
};
} // namespace ghaf::AudioControl
