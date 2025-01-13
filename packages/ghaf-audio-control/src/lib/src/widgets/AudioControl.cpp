/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
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

AudioControl::AudioControl(std::shared_ptr<IAudioControlBackend> backend, const std::vector<std::string>& appVmsList)
    : Gtk::Box(Gtk::ORIENTATION_VERTICAL)
    , m_audioControl(std::move(backend))
    , m_sinksModel(DeviceListModel::create("Speakers"))
    , m_sinks(m_sinksModel)
    , m_sourcesModel(DeviceListModel::create("Microphones"))
    , m_sources(m_sourcesModel)
{
    set_halign(Gtk::Align::ALIGN_START);
    set_valign(Gtk::Align::ALIGN_START);

    for (const auto& appVm : appVmsList)
        m_appList.addVm(appVm);

    init();
}

void AudioControl::init()
{
    if (m_audioControl)
    {
        pack_start(m_sinks);
        pack_start(m_sources);
        pack_start(m_appList);

        m_connections += m_audioControl->onSinksChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSinksChanged));
        m_connections += m_audioControl->onSourcesChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSourcesChanged));
        m_connections += m_audioControl->onSinkInputsChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSinkInputsChanged));
        // m_connections += m_audioControl->onSourceOutputsChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSourcesOutputsChanged));
        m_connections += m_audioControl->onError().connect(sigc::mem_fun(*this, &AudioControl::onPulseError));

        show_all_children();

        m_audioControl->start();
    }
    else
    {
        onPulseError("No audio backend");
    }

    auto cssProvider = Gtk::CssProvider::create();
    cssProvider->load_from_data(CssStyle);

    auto style_context = Gtk::StyleContext::create();
    style_context->add_provider_for_screen(Gdk::Screen::get_default(), cssProvider, GTK_STYLE_PROVIDER_PRIORITY_USER);
}

void AudioControl::onPulseSinksChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info)
{
    if (info.eventType == IAudioControlBackend::EventType::Add)
        m_sinksModel->addDevice(std::move(info.ptr));
}

void AudioControl::onPulseSourcesChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info)
{
    if (info.eventType == IAudioControlBackend::EventType::Add)
        m_sourcesModel->addDevice(std::move(info.ptr));
}

void AudioControl::onPulseSinkInputsChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info)
{
    OnPulseDeviceChanged(info.eventType, info.index, std::move(info.ptr), m_appList);
}

void AudioControl::onPulseSourcesOutputsChanged(IAudioControlBackend::OnSignalMapChangeSignalInfo info)
{
    OnPulseDeviceChanged(info.eventType, info.index, std::move(info.ptr), m_appList);
}

void AudioControl::onPulseError(std::string_view error)
{
    m_connections.clear();
    m_audioControl->stop();

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
