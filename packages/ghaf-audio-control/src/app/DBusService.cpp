/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include "DBusService.hpp"

#include <GhafAudioControl/utils/Logger.hpp>

#include <giomm/dbuserror.h>
#include <giomm/dbusownname.h>

using namespace ghaf::AudioControl;

namespace
{

namespace MethodName
{

constexpr auto Open = "Open";
constexpr auto Toggle = "Toggle";

constexpr auto SubscribeToDeviceUpdatedSignal = "SubscribeToDeviceUpdatedSignal";
constexpr auto UnsubscribeFromDeviceUpdatedSignal = "UnsubscribeFromDeviceUpdatedSignal";

constexpr auto SetDeviceVolume = "SetDeviceVolume";
constexpr auto SetDeviceMute = "SetDeviceMute";

} // namespace MethodName

namespace SignalName
{

constexpr auto DeviceUpdated = "DeviceUpdated";

}

namespace SystemTrayWatcher
{

constexpr auto ServiceName = "org.kde.StatusNotifierWatcher";
constexpr auto ObjectPath = "/StatusNotifierWatcher";
constexpr auto InterfaceName = "org.kde.StatusNotifierWatcher";

constexpr auto RegisterStatusNotifierMethodName = "RegisterStatusNotifierItem";

constexpr auto NewIconSignalName = "NewIcon";

} // namespace SystemTrayWatcher

namespace StatusNotifierItem
{

constexpr auto ObjectPath = "/StatusNotifierItem";
constexpr auto InterfaceName = "org.kde.StatusNotifierItem";

} // namespace StatusNotifierItem

namespace AudioControlService
{

constexpr auto ObjectPath = "/org/ghaf/Audio";
constexpr auto InterfaceName = "org.ghaf.Audio";

} // namespace AudioControlService

const auto IntrospectionXml = std::format(R"xml(
    <node>
        <interface name='org.ghaf.Audio'>
            <method name='{}' />    <!-- Open -->
            <method name='{}' />    <!-- Toggle -->

            <!--
                Enum: DeviceType
                Values:
                    - 0: Sink
                    - 1: Source
                    - 2: SinkInput
                    - 3: SourceOutput

                Enum: EventType
                Values:
                    - 0: Add
                    - 1: Update
                    - 2: Delete
            -->

            <method name='{}' />    <!-- SubscribeToDeviceUpdatedSignal -->
            <method name='{}' />    <!-- UnsubscribeFromDeviceUpdatedSignal -->

            <method name='{}'>      <!-- SetDeviceVolume -->
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum -->
                <arg name='volume' type='i' direction='in' />       <!-- min: 0, max: 100 -->

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <method name='{}'>      <!-- SetDeviceMute -->
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum -->
                <arg name='mute' type='b' direction='in' />

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <signal name='{}'>      <!-- DeviceUpdated -->
                <arg name='id' type='i' />
                <arg name='type' type='i' />                         <!-- See DeviceType enum -->
                <arg name='name' type='s' />
                <arg name='volume' type='i' />                       <!-- min: 0, max: 100 -->
                <arg name='isMuted' type='b' />
                <arg name='event' type='i' />                        <!-- See EventType enum -->
            </signal>
        </interface>

        <interface name="org.kde.StatusNotifierItem">
            <property name="Category" type="s" access="read"/>
            <property name="Id" type="s" access="read"/>
            <property name="Status" type="s" access="read"/>
            <property name="Title" type="s" access="read"/>
            <property name="IconName" type="s" access="read"/>
            <property name="Menu" type="o" access="read"/>

            <method name="Activate">
                <arg type="i" name="x" direction="in"/>
                <arg type="i" name="y" direction="in"/>
            </method>
        </interface>
    </node>
)xml",
                                          MethodName::Open, MethodName::Toggle, MethodName::SubscribeToDeviceUpdatedSignal,
                                          MethodName::UnsubscribeFromDeviceUpdatedSignal, MethodName::SetDeviceVolume, MethodName::SetDeviceMute,
                                          SignalName::DeviceUpdated);

DBusService::DeviceType intToDeviceType(int value)
{
    switch (value)
    {
    case 0:
        return DBusService::DeviceType::Sink;
    case 1:
        return DBusService::DeviceType::Source;
    case 2:
        return DBusService::DeviceType::SinkInput;
    case 3:
        return DBusService::DeviceType::SourceOutput;

    default:
        throw std::runtime_error{std::format("'type' field has an unsupported value: {}", value)};
    }
}

int deviceTypeToInt(DBusService::DeviceType type)
{
    return static_cast<int>(type);
}

auto createEmptyResponse()
{
    return Glib::VariantContainerBase::create_tuple(std::vector<Glib::VariantBase>{});
};

auto createResultOkResponse()
{
    return Glib::VariantContainerBase::create_tuple(Glib::Variant<int>::create(0));
};

} // namespace

DBusService::DBusService()
    : m_interfaceVtable(sigc::mem_fun(*this, &DBusService::onMethodCall), sigc::mem_fun(*this, &DBusService::onPropertyGet),
                        sigc::mem_fun(*this, &DBusService::onPropertySet))
    , m_introspectionData(Gio::DBus::NodeInfo::create_for_xml(IntrospectionXml))
    , m_connectionId(Gio::DBus::own_name(Gio::DBus::BUS_TYPE_SESSION, AudioControlService::InterfaceName, sigc::mem_fun(*this, &DBusService::onBusAcquired),
                                         sigc::mem_fun(*this, &DBusService::onNameAcquired), sigc::mem_fun(*this, &DBusService::onNameLost)))
{
    m_methodHandlers = {
        {MethodName::Open, sigc::mem_fun(*this, &DBusService::onOpenMethod)},
        {MethodName::Toggle, sigc::mem_fun(*this, &DBusService::onToggleMethod)},

        {MethodName::SubscribeToDeviceUpdatedSignal, sigc::mem_fun(*this, &DBusService::onSubscribeToDeviceUpdatedSignalMethod)},
        {MethodName::UnsubscribeFromDeviceUpdatedSignal, sigc::mem_fun(*this, &DBusService::onUnsubscribeFromDeviceUpdatedSignalMethod)},

        {MethodName::SetDeviceVolume, sigc::mem_fun(*this, &DBusService::onSetDeviceVolumeMethod)},
        {MethodName::SetDeviceMute, sigc::mem_fun(*this, &DBusService::onSetDeviceMuteMethod)},
    };
}

DBusService::~DBusService()
{
    Gio::DBus::unown_name(m_connectionId);
}

void DBusService::sendDeviceInfo(DeviceIndex index, DeviceType type, const std::string& name, DeviceVolume volume, bool isMuted, DeviceEventType eventType)
{
    const Glib::VariantContainerBase args = Glib::VariantContainerBase::create_tuple({
        Glib::Variant<int>::create(index),                      // id
        Glib::Variant<int>::create(deviceTypeToInt(type)),      // type
        Glib::Variant<Glib::ustring>::create(name.c_str()),     // name
        Glib::Variant<int>::create(volume.getPercents()),       // volume
        Glib::Variant<bool>::create(isMuted),                   // isMuted
        Glib::Variant<int>::create(static_cast<int>(eventType)) // event
    });

    Logger::debug("DBusService::sendDeviceInfo: {}", args.print(true).c_str());

    Gio::DBus::Connection::get_sync(Gio::DBus::BUS_TYPE_SESSION)
        ->emit_signal(AudioControlService::ObjectPath, AudioControlService::InterfaceName, SignalName::DeviceUpdated, "", args);
}

void DBusService::registerSystemTrayIcon(const Glib::ustring& iconName)
{
    m_iconName = iconName;

    try
    {
        auto connection = Gio::DBus::Connection::get_sync(Gio::DBus::BUS_TYPE_SESSION);

        auto args = Glib::VariantContainerBase::create_tuple(
            std::vector<Glib::VariantBase>{Glib::Variant<Glib::ustring>::create(StatusNotifierItem::ObjectPath)});
        connection->call_sync(SystemTrayWatcher::ObjectPath,
                              SystemTrayWatcher::InterfaceName,
                              SystemTrayWatcher::RegisterStatusNotifierMethodName,
                              args,
                              SystemTrayWatcher::ServiceName);

        auto signalArgs = Glib::VariantContainerBase::create_tuple(
            std::vector<Glib::VariantBase>{Glib::Variant<Glib::ustring>::create(StatusNotifierItem::ObjectPath)});
        connection->emit_signal(SystemTrayWatcher::ObjectPath,
                                SystemTrayWatcher::InterfaceName,
                                SystemTrayWatcher::NewIconSignalName,
                                SystemTrayWatcher::ServiceName,
                                signalArgs);

        Logger::debug("Registered StatusNotifierItem successfully.");
    }
    catch (const Glib::Error& ex)
    {
        Logger::error("Error registering StatusNotifierItem: {}", ex.what().c_str());
    }
}

void DBusService::onBusAcquired([[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name)
{
    Logger::debug("The bus for a name: {} is acquired, registering...", name.c_str());

    connection->register_object(AudioControlService::ObjectPath, m_introspectionData->lookup_interface(AudioControlService::InterfaceName), m_interfaceVtable);
    connection->register_object(StatusNotifierItem::ObjectPath, m_introspectionData->lookup_interface(StatusNotifierItem::InterfaceName), m_interfaceVtable);
}

void DBusService::onNameAcquired([[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name)
{
    Logger::debug("The DBus service with a name {} has been registered", name.c_str());
}

void DBusService::onNameLost([[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name)
{
    Logger::error("Couldn't register service for a name: {}", name.c_str());
}

void DBusService::onMethodCall([[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection, [[maybe_unused]] const Glib::ustring& sender,
                               [[maybe_unused]] const Glib::ustring& objectPath, [[maybe_unused]] const Glib::ustring& interfaceName,
                               const Glib::ustring& methodName, const Glib::VariantContainerBase& parameters,
                               const Glib::RefPtr<Gio::DBus::MethodInvocation>& invocation)
{
    Logger::debug("Invokated method: {}{} on the interface: {} from a client: {}",
                  methodName.c_str(),
                  parameters.print(true).c_str(),
                  interfaceName.c_str(),
                  sender.c_str());

    try
    {
        if (objectPath == AudioControlService::ObjectPath)
        {
            if (auto method = m_methodHandlers.find(methodName); method != m_methodHandlers.end())
                invocation->return_value(m_methodHandlers[methodName](parameters));
            else
                throw std::runtime_error{std::format("Unsupported method: {}", methodName.c_str())};
        }
        else if (objectPath == StatusNotifierItem::ObjectPath)
        {
            onToggleMethod(parameters);
        }
        else
        {
            throw std::runtime_error{std::format("Unsupported method: {} from unsupported objectPath: {}", methodName.c_str(), objectPath.c_str())};
        }
    }
    catch (const std::exception& ex)
    {
        Logger::error(ex.what());
        invocation->return_error(Gio::DBus::Error(Gio::DBus::Error::FAILED, ex.what()));
    }
}

void DBusService::onPropertyGet(Glib::VariantBase& property, [[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection,
                                [[maybe_unused]] const Glib::ustring& sender, [[maybe_unused]] const Glib::ustring& objectPath,
                                const Glib::ustring& interfaceName, const Glib::ustring& propertyName)
{
    static const std::map<Glib::ustring, Glib::ustring> propertyMap = {{"Category", "ApplicationStatus"},
                                                                       {"Id", "Ghaf Audio Control"},
                                                                       {"Status", "Active"},
                                                                       {"Title", "Ghaf Audio Control"},
                                                                       {"IconName", m_iconName.value_or("")},
                                                                       {"Menu", ""}};

    if (auto it = propertyMap.find(propertyName); it != propertyMap.end())
        property = Glib::Variant<Glib::ustring>::create(it->second);
    else
        property = Glib::VariantBase();

    Logger::debug("onPropertyGet: property: {} on the interface: {} returns: {}", propertyName.c_str(), interfaceName.c_str(), property.print(true).c_str());
}

bool DBusService::onPropertySet([[maybe_unused]] const Glib::RefPtr<Gio::DBus::Connection>& connection, [[maybe_unused]] const Glib::ustring& sender,
                                [[maybe_unused]] const Glib::ustring& objectPath, [[maybe_unused]] const Glib::ustring& interfaceName,
                                [[maybe_unused]] const Glib::ustring& propertyName, [[maybe_unused]] const Glib::VariantBase& value)
{
    Logger::debug("onPropertySet");
    return false;
}

DBusService::MethodResult DBusService::onOpenMethod([[maybe_unused]] const MethodParameters& parameters)
{
    m_openSignal();
    return createEmptyResponse();
}

DBusService::MethodResult DBusService::onToggleMethod([[maybe_unused]] const MethodParameters& parameters)
{
    m_toggleSignal();
    return createEmptyResponse();
}

DBusService::MethodResult DBusService::onSubscribeToDeviceUpdatedSignalMethod([[maybe_unused]] const MethodParameters& parameters)
{
    m_subscribeToDeviceUpdatedSignal();
    return createEmptyResponse();
}

DBusService::MethodResult DBusService::onUnsubscribeFromDeviceUpdatedSignalMethod([[maybe_unused]] const MethodParameters& parameters)
{
    // m_unsubscribeToDeviceUpdatedSignal();
    return createEmptyResponse();
}

DBusService::MethodResult DBusService::onSetDeviceVolumeMethod(const MethodParameters& parameters)
{
    Glib::Variant<int> id;
    Glib::Variant<int> type;
    Glib::Variant<int> volume;

    parameters.get_child(id, 0);
    parameters.get_child(type, 1);
    parameters.get_child(volume, 2);

    m_setDeviceVolumeSignal(id.get(), intToDeviceType(type.get()), Volume::fromPercents(static_cast<unsigned long>(volume.get())));

    return createResultOkResponse();
}

DBusService::MethodResult DBusService::onSetDeviceMuteMethod(const MethodParameters& parameters)
{
    Glib::Variant<int> id;
    Glib::Variant<int> type;
    Glib::Variant<bool> mute;

    parameters.get_child(id, 0);
    parameters.get_child(type, 1);
    parameters.get_child(mute, 2);

    m_setDeviceMuteSignal(id.get(), intToDeviceType(type.get()), mute.get());

    return createResultOkResponse();
}
