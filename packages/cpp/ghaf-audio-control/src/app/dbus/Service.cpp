/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <ghaf-audio-control-app/dbus/Service.hpp>

#include <GhafAudioControl/utils/Check.hpp>
#include <GhafAudioControl/utils/Logger.hpp>

#include <giomm/dbuserror.h>
#include <giomm/dbusownname.h>

using namespace ghaf::AudioControl;

namespace ghaf::AudioControl::dbus
{

namespace
{

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

namespace MethodName
{

constexpr auto Activate = "Activate";

}

} // namespace StatusNotifierItem

constexpr auto IntrospectionXml = R"xml(
    <node>
        <interface name='org.ghaf.Audio'>
            <method name='Open' />
            <method name='Toggle' />

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

            <method name='SubscribeToDeviceUpdatedSignal' />
            <method name='UnsubscribeFromDeviceUpdatedSignal' />

            <method name='SetDeviceVolume'>
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum -->
                <arg name='volume' type='i' direction='in' />       <!-- min: 0, max: 100 -->

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <method name='SetDeviceMute'>
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum -->
                <arg name='mute' type='b' direction='in' />

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <method name='MakeDeviceDefault'>
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum. Only a Sink or a Source -->

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <signal name='DeviceUpdated'>
                <arg name='id' type='i' />
                <arg name='type' type='i' />                         <!-- See DeviceType enum -->
                <arg name='name' type='s' />
                <arg name='volume' type='i' />                       <!-- min: 0, max: 100 -->
                <arg name='isMuted' type='b' />
                <arg name='isDefault' type='b' />                    <!-- Makes sense only for a Sink and a Source -->
                <arg name='event' type='i' />                        <!-- See EventType enum -->
            </signal>
        </interface>

        <interface name="org.kde.StatusNotifierItem">
            <property name="Category" type="s" access="read"/>
            <property name="Id" type="s" access="read"/>
            <property name="Status" type="s" access="read"/>
            <property name="Title" type="s" access="read"/>
            <property name="IconName" type="s" access="read"/>
            <property name="IconThemePath" type="s" access="read"/>
            <property name="Menu" type="o" access="read"/>

            <method name="Activate">
                <arg type="i" name="x" direction="in"/>
                <arg type="i" name="y" direction="in"/>
            </method>
        </interface>
    </node>
)xml";

} // namespace

DBusService::DBusService()
    : m_interfaceVtable(sigc::mem_fun(*this, &DBusService::onMethodCall), sigc::mem_fun(*this, &DBusService::onPropertyGet),
                        sigc::mem_fun(*this, &DBusService::onPropertySet))
    , m_introspectionData(Gio::DBus::NodeInfo::create_for_xml(IntrospectionXml))
{
}

DBusService::~DBusService()
{
    Gio::DBus::unown_name(m_connectionId);
}

void DBusService::addInterface(Interface::Ptr interface)
{
    CheckNullPtr(interface);

    m_connectionId = Gio::DBus::own_name(Gio::DBus::BUS_TYPE_SESSION,
                                         interface->getName().data(),
                                         sigc::mem_fun(*this, &DBusService::onBusAcquired),
                                         sigc::mem_fun(*this, &DBusService::onNameAcquired),
                                         sigc::mem_fun(*this, &DBusService::onNameLost));

    Check(m_interfaces.empty(), "DBusService supports only 1 service by now");

    for (const auto& signal : interface->getSignals())
    {
        signal->onSignal().connect(
            [signal, interface](const auto& client, const auto& args)
            {
                Logger::debug("DBusService::sendDeviceInfo: {} to the client: {}", args.print(true).c_str(), client.c_str());

                Gio::DBus::Connection::get_sync(Gio::DBus::BUS_TYPE_SESSION)
                    ->emit_signal(interface->getObjectPath(), interface->getName(), signal->getName(), client, args);
            });
    }

    m_interfaces[std::string(interface->getName())] = std::move(interface);
}

void DBusService::registerSystemTrayIcon(const Glib::ustring& iconName)
{
    m_iconName = iconName;

    try
    {
        auto connection = Gio::DBus::Connection::get_sync(Gio::DBus::BUS_TYPE_SESSION);
        connection->register_object(StatusNotifierItem::ObjectPath, m_introspectionData->lookup_interface(StatusNotifierItem::InterfaceName), m_interfaceVtable);

        auto args = Glib::VariantContainerBase::create_tuple(
            std::vector<Glib::VariantBase>{Glib::Variant<Glib::ustring>::create(StatusNotifierItem::ObjectPath)});
        connection->call_sync(SystemTrayWatcher::ObjectPath,
                              SystemTrayWatcher::InterfaceName,
                              SystemTrayWatcher::RegisterStatusNotifierMethodName,
                              args,
                              SystemTrayWatcher::ServiceName);

        {
            auto signalArgs = Glib::VariantContainerBase::create_tuple(
                std::vector<Glib::VariantBase>{Glib::Variant<Glib::ustring>::create(StatusNotifierItem::ObjectPath)});
            connection->emit_signal(SystemTrayWatcher::ObjectPath,
                                    SystemTrayWatcher::InterfaceName,
                                    SystemTrayWatcher::NewIconSignalName,
                                    SystemTrayWatcher::ServiceName,
                                    signalArgs);
        }

        Logger::debug("Registered StatusNotifierItem successfully.");
    }
    catch (const Glib::Error& ex)
    {
        Logger::error("Error registering StatusNotifierItem: {}", ex.what().c_str());
    }
}

void DBusService::onBusAcquired(const Glib::RefPtr<Gio::DBus::Connection>& connection, const Glib::ustring& name)
{
    Logger::debug("The bus for a name: {} is acquired, registering...", name.c_str());
    const auto interface = m_interfaces.begin()->second;
    const auto objectPath = interface->getObjectPath();
    const auto interfaceName = interface->getName();

    Logger::debug("Registering object {} for interface: {}", objectPath, interfaceName);
    connection->register_object(objectPath, m_introspectionData->lookup_interface(interfaceName), m_interfaceVtable);
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
        if (auto it = m_interfaces.find(interfaceName); it != m_interfaces.end())
        {
            auto resultVariant = it->second->invokeMethod(methodName, sender, parameters);

            if (auto* methodResult = std::get_if<BaseMethod::MethodResult>(&resultVariant))
            {
                Logger::debug("Invokated method: {}{} on the interface: {} from a client: {} returns rusult: {}",
                              methodName.c_str(),
                              parameters.print(true).c_str(),
                              interfaceName.c_str(),
                              sender.c_str(),
                              methodResult->print(true).c_str());
                invocation->return_value(*methodResult);
            }
            else
            {
                const auto error = std::get<Glib::Error>(resultVariant);
                Logger::debug("Invokated method: {}{} on the interface: {} from a client: {} returns error: {}",
                              methodName.c_str(),
                              parameters.print(true).c_str(),
                              interfaceName.c_str(),
                              sender.c_str(),
                              error.what().c_str());
                invocation->return_error(error);
            }
        }
        else
        {
            throw std::runtime_error{std::format("Unsupported interface: {}", interfaceName.c_str())};
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
    static const std::map<Glib::ustring, Glib::ustring> propertyMap = {
        {"Category", "ApplicationStatus"},
        {"Id", "Ghaf Audio Control"},
        {"Status", "Active"},
        {"Title", "Ghaf Audio Control"},
        {"IconName", m_iconName.value_or("")},
        {"Menu", ""},
        {"IconThemePath", ""},
    };

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

} // namespace ghaf::AudioControl::dbus
