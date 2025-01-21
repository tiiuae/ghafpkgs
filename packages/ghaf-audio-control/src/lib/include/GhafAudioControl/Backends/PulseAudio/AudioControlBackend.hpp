/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include <GhafAudioControl/IAudioControlBackend.hpp>
#include <GhafAudioControl/utils/RaiiWrap.hpp>

#include <pulse/glib-mainloop.h>
#include <pulse/introspect.h>
#include <pulse/thread-mainloop.h>

namespace ghaf::AudioControl::Backend::PulseAudio
{

class AudioControlBackend final : public IAudioControlBackend
{
public:
    explicit AudioControlBackend(std::string pulseAudioServerAddress);
    ~AudioControlBackend() override = default;

    const std::string& getServerAddress() const noexcept
    {
        return m_serverAddress;
    }

    void start() override;
    void stop() override;

    void setDeviceVolume(IDevice::IntexT index, IDevice::Type type, Volume volume) override;
    void setDeviceMute(IDevice::IntexT index, IDevice::Type type, bool mute) override;

    void makeDeviceDefault(IDevice::IntexT index, IDevice::Type type) override;

    std::vector<IAudioControlBackend::IDevice::Ptr> getAllDevices() const override;

    Sinks::OnChangeSignal onSinksChanged() const override
    {
        return m_sinks.onChange();
    }

    Sources::OnChangeSignal onSourcesChanged() const override
    {
        return m_sources.onChange();
    }

    SinkInputs::OnChangeSignal onSinkInputsChanged() const override
    {
        return m_sinkInputs.onChange();
    }

    SourceOutputs::OnChangeSignal onSourceOutputsChanged() const override
    {
        return m_sourceOutputs.onChange();
    }

    OnErrorSignal onError() const override
    {
        return m_onError;
    }

private:
    static void subscribeCallback(pa_context* context, pa_subscription_event_type_t type, uint32_t index, void* data);
    static void contextStateCallback(pa_context* context, void* data);
    static void sinkInfoCallback(pa_context* context, const pa_sink_info* info, int eol, void* data);
    static void sourceInfoCallback(pa_context* context, const pa_source_info* info, int eol, void* data);
    static void sinkInputInfoCallback(pa_context* context, const pa_sink_input_info* info, int eol, void* data);
    static void sourceOutputInfoCallback(pa_context* context, const pa_source_output_info* info, int eol, void* data);
    static void serverInfoCallback(pa_context* context, const pa_server_info* info, void* data);
    static void cardInfoCallback(pa_context* context, const pa_card_info* info, int eol, void* data);

    void onSinkInfo(const pa_sink_info& info);
    void deleteSink(Sinks::IndexT index);

    void onSourceInfo(const pa_source_info& info);
    void deleteSource(Sources::IndexT index);

    void onSinkInputInfo(const pa_sink_input_info& info);
    void deleteSinkInput(SinkInputs::IndexT index);

    void onSourceOutputInfo(const pa_source_output_info& info);
    void deleteSourceOutput(SourceOutputs::IndexT index);

    void onServerInfo(const pa_server_info& info);

private:
    Sinks m_sinks;
    Sources m_sources;
    SinkInputs m_sinkInputs;
    SourceOutputs m_sourceOutputs;

    OnErrorSignal m_onError;

    std::string m_serverAddress;
    std::string m_defaultSinkName;
    std::string m_defaultSourceName;

    RaiiWrap<pa_glib_mainloop*> m_mainloop;
    RaiiWrap<pa_mainloop_api*> m_mainloopApi;
    std::optional<RaiiWrap<pa_context*>> m_context;
};

} // namespace ghaf::AudioControl::Backend::PulseAudio
