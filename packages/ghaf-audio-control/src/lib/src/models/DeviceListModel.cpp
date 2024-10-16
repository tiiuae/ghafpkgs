/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/models/DeviceListModel.hpp>

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

DeviceListModel::DeviceListModel(std::string name, std::string namePrefix)
    : Glib::ObjectBase(typeid(DeviceListModel))
    , m_name(*this, "name", std::move(name))
    , m_namePrefix(std::move(namePrefix))
    , m_devices(Gio::ListStore<DeviceModel>::create())
{
}

Glib::RefPtr<DeviceListModel> DeviceListModel::create(std::string name, std::string namePrefix)
{
    return Glib::RefPtr<DeviceListModel>(new DeviceListModel(std::move(name), std::move(namePrefix)));
}

void DeviceListModel::addDevice(IAudioControlBackend::IDevice::Ptr sink)
{
    const Index deviceIndex = sink->getIndex();

    if (GetDeviceIndex(*m_devices.get(), deviceIndex))
    {
        Logger::error("AppVmModel: ignore doubling");
        return;
    }

    m_devices->append(DeviceModel::create(sink));

    m_deviceConnections[deviceIndex] = sink->onDelete().connect(
        [this, deviceIndex]
        {
            if (const auto index = GetDeviceIndex(*m_devices.get(), deviceIndex))
                m_devices->remove(*index);
            else
                Logger::error("DevicesModel::addSink couldn't found sink");

            m_deviceConnections.erase(deviceIndex);
        });
}

} // namespace ghaf::AudioControl
