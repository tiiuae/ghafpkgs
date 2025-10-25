/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/widgets/AudioControl.hpp>

#include <GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp>
#include <GhafAudioControl/utils/Logger.hpp>

#include <gtkmm/grid.h>
#include <gtkmm/image.h>
#include <gtkmm/scale.h>
#include <gtkmm/switch.h>
#include <gtkmm/volumebutton.h>

#include <glibmm/binding.h>

namespace ghaf::AudioControl
{

constexpr auto CssStyle = "button#AppVmNameButton { background-color: transparent; border: none; font-weight: bold; }"
                          "box#DeviceWidget { border-radius: 15px; }"
                          "label#EmptyListName { border-radius: 15px; min-height: 40px; }"
                          "*:selected { background-color: transparent; color: inherit; box-shadow: none; outline: none; }";

template<class IndexT, class DevicePtrT>
void OnPulseDeviceChanged(IAudioControlBackend::EventType eventType, IndexT index, DevicePtrT device, AppList& appList)
{
    std::string deviceType;

    if constexpr (std::is_same<DevicePtrT, IAudioControlBackend::Sinks::PtrT>())
    {
        deviceType = "sink";
    }
    else if constexpr (std::is_same<DevicePtrT, IAudioControlBackend::Sources::PtrT>())
    {
        deviceType = "source";
    }
    else if constexpr (std::is_same<DevicePtrT, IAudioControlBackend::SinkInputs::PtrT>())
    {
        deviceType = "sinkInput";
    }
    else if constexpr (std::is_same<DevicePtrT, IAudioControlBackend::SourceOutputs::PtrT>())
    {
        deviceType = "sourceOutput";
    }
    else
    {
        static_assert(true, "Unknow type");
    }

    switch (eventType)
    {
    case IAudioControlBackend::EventType::Add:
        Logger::debug("OnPulseDeviceChanged: ADD {}: {}", deviceType, device->toString());
        appList.addDevice(std::move(device));
        break;

    case IAudioControlBackend::EventType::Update:
        Logger::debug("OnPulseDeviceChanged: UPDATE {}: {}", deviceType, device->toString());
        break;

    case IAudioControlBackend::EventType::Delete:
        Logger::debug("OnPulseDeviceChanged: DELETE {} with index: {}", deviceType, index);
        break;
    }
}

AudioControl::AudioControl(const std::vector<std::string>& appVmsList, bool allowMultipleStreamsPerVm)
    : Gtk::Box(Gtk::ORIENTATION_VERTICAL)
    , m_allowMultipleStreamsPerVm(allowMultipleStreamsPerVm)
    , m_sinksModel(DeviceListModel::create("Speakers"))
    , m_sinks(m_sinksModel)
    , m_sourcesModel(DeviceListModel::create("Microphones"))
    , m_sources(m_sourcesModel)
    , m_metaSinkInputModel(DeviceListModel::create("Meta"))
    , m_metaSinkInputs(m_metaSinkInputModel)
{
    set_halign(Gtk::Align::ALIGN_START);
    set_valign(Gtk::Align::ALIGN_START);

    for (const auto& appVm : appVmsList)
        m_appList.addVm(appVm);

    init();
}

void AudioControl::sendDeviceInfoUpdate(const IAudioControlBackend::OnSignalMapChangeSignalInfo& info)
{
    switch (info.eventType)
    {
    case IAudioControlBackend::EventType::Add:
    {
        switch (info.type)
        {
        case IAudioControlBackend::IDevice::Type::Sink:
        {
            m_sinksModel->addDevice(info.ptr);
            break;
        }

        case IAudioControlBackend::IDevice::Type::Source:
        {
            m_sourcesModel->addDevice(info.ptr);
            break;
        }

        case IAudioControlBackend::IDevice::Type::SinkInput:
        {
            if (m_allowMultipleStreamsPerVm)
                m_appList.addDevice(info.ptr);

            break;
        }

        case IAudioControlBackend::IDevice::Type::SourceOutput:

        case IAudioControlBackend::IDevice::Type::Meta:
            if (m_allowMultipleStreamsPerVm)
                m_metaSinkInputModel->addDevice(info.ptr);
            else
                m_appList.addDevice(info.ptr);

            break;
        }

        Logger::debug("OnPulseDeviceChanged: ADD {}", info.ptr->toString());
        break;
    }

    case IAudioControlBackend::EventType::Update:
        Logger::debug("OnPulseDeviceChanged: UPDATE {}", info.ptr->toString());
        break;

    case IAudioControlBackend::EventType::Delete:
        Logger::debug("OnPulseDeviceChanged: DELETE index: {}", info.index);
        break;
    }
}

void AudioControl::sendError(std::string_view error)
{
    onPulseError(error);
}

void AudioControl::init()
{
    pack_start(m_sinks);
    pack_start(m_sources);
    pack_start(m_appList);

    if (m_allowMultipleStreamsPerVm)
        pack_start(m_metaSinkInputs);

    show_all_children();

    auto cssProvider = Gtk::CssProvider::create();
    cssProvider->load_from_data(CssStyle);

    auto style_context = Gtk::StyleContext::create();
    style_context->add_provider_for_screen(Gdk::Screen::get_default(), cssProvider, GTK_STYLE_PROVIDER_PRIORITY_USER);
}

void AudioControl::onPulseError(std::string_view error)
{
    Logger::error(error);

    m_appList.removeAllApps();
    remove(m_appList);

    auto* errorLabel = Gtk::make_managed<Gtk::Label>();
    errorLabel->set_label(error.data());
    errorLabel->show_all();

    pack_start(*errorLabel);

    show_all_children();
}

} // namespace ghaf::AudioControl
