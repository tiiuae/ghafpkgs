/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/AudioControl.hpp>

#include <GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp>
#include <GhafAudioControl/utils/Logger.hpp>

#include <gtkmm/grid.h>
#include <gtkmm/image.h>
#include <gtkmm/scale.h>
#include <gtkmm/switch.h>
#include <gtkmm/volumebutton.h>

#include <glibmm/binding.h>

#include <format>

namespace ghaf::AudioControl
{

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
        Logger::debug(std::format("OnPulseDeviceChanged: ADD {}: {}", deviceType, device->toString()));
        appList.addDevice(std::move(device));
        break;

    case IAudioControlBackend::EventType::Update:
        Logger::debug(std::format("OnPulseDeviceChanged: UPDATE {}: {}", deviceType, device->toString()));
        break;

    case IAudioControlBackend::EventType::Delete:
        Logger::debug(std::format("OnPulseDeviceChanged: DELETE {} with index: {}", deviceType, index));
        break;
    }
}

AudioControl::AudioControl(std::unique_ptr<IAudioControlBackend> backend)
    : Gtk::Box(Gtk::ORIENTATION_HORIZONTAL)
    , m_audioControl(std::move(backend))
{
    init();
}

void AudioControl::init()
{
    if (m_audioControl)
    {
        pack_start(m_appList);

        // m_connections.add(m_audioControl->onSinksChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSinksChanged)));
        // m_connections.add(m_audioControl->onSourcesChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSourcesChanged)));
        m_connections.add(m_audioControl->onSinkInputsChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSinkInputsChanged)));
        // m_connections.add(m_audioControl->onSourceOutputsChanged().connect(sigc::mem_fun(*this, &AudioControl::onPulseSourcesOutputsChanged)));
        m_connections.add(m_audioControl->onError().connect(sigc::mem_fun(*this, &AudioControl::onPulseError)));

        show_all_children();

        m_audioControl->start();
    }
    else
    {
        onPulseError("No audio backend");
    }
}

void AudioControl::onPulseSinksChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::Sinks::IndexT extIndex,
                                       IAudioControlBackend::Sinks::PtrT sink)
{
    OnPulseDeviceChanged(eventType, extIndex, std::move(sink), m_appList);
    show_all_children();
}

void AudioControl::onPulseSourcesChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::Sources::IndexT extIndex,
                                         IAudioControlBackend::Sources::PtrT source)
{
    OnPulseDeviceChanged(eventType, extIndex, std::move(source), m_appList);
    show_all_children();
}

void AudioControl::onPulseSinkInputsChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::SinkInputs::IndexT extIndex,
                                            IAudioControlBackend::SinkInputs::PtrT sinkInput)
{
    OnPulseDeviceChanged(eventType, extIndex, std::move(sinkInput), m_appList);
    show_all_children();
}

void AudioControl::onPulseSourcesOutputsChanged(IAudioControlBackend::EventType eventType, IAudioControlBackend::SourceOutputs::IndexT extIndex,
                                                IAudioControlBackend::SourceOutputs::PtrT sourceOutput)
{
    OnPulseDeviceChanged(eventType, extIndex, std::move(sourceOutput), m_appList);
    show_all_children();
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
