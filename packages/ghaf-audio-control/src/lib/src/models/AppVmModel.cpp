/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/models/AppVmModel.hpp>

#include <GhafAudioControl/utils/Logger.hpp>

namespace ghaf::AudioControl
{

namespace
{

std::optional<guint> GetDeviceIndex(const Gio::ListStore<DeviceModel>& list, Index index)
{
    const guint size = list.get_n_items();

    for (guint i = 0; i < size; i++)
    {
        if (list.get_item(i)->getIndex() == index)
            return i;
    }

    return std::nullopt;
}

} // namespace

AppVmModel::AppVmModel(AppIdType id)
    : Glib::ObjectBase(typeid(AppVmModel))
    , m_devices(Gio::ListStore<DeviceModel>::create())
    , m_appName(*this, "m_appName", std::move(id))
{
}

Glib::RefPtr<AppVmModel> AppVmModel::create(AppIdType id)
{
    return Glib::RefPtr<AppVmModel>(new AppVmModel(std::move(id)));
}

int AppVmModel::compare(const Glib::RefPtr<const AppVmModel>& a, const Glib::RefPtr<const AppVmModel>& b)
{
    if (!a || !b)
        return 0;

    return a->m_appName.get_value().compare(b->m_appName.get_value());
}

void AppVmModel::addSinkInput(IAudioControlBackend::ISinkInput::Ptr sinkInput)
{
    const Index deviceIndex = sinkInput->getIndex();

    if (GetDeviceIndex(*m_devices.get(), deviceIndex))
    {
        Logger::error("AppVmModel: ignore doubling");
        return;
    }

    m_devices->append(DeviceModel::create(sinkInput));

    m_deviceConnections[deviceIndex] = sinkInput->onDelete().connect(
        [this, deviceIndex]
        {
            if (const auto index = GetDeviceIndex(*m_devices.get(), deviceIndex))
                m_devices->remove(*index);
            else
                Logger::error("AppVmModel::addSinkInput couldn't found sinkInput");

            m_deviceConnections.erase(deviceIndex);
        });
}

void AppVmModel::deleteSinkInput(const IAudioControlBackend::IDevice::Ptr& sinkInput)
{
}

} // namespace ghaf::AudioControl
