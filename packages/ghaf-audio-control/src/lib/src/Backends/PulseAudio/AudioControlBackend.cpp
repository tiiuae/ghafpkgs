/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/AudioControlBackend.hpp>

#include <GhafAudioControl/Backends/PulseAudio/Helpers.hpp>
#include <GhafAudioControl/Backends/PulseAudio/Sink.hpp>
#include <GhafAudioControl/Backends/PulseAudio/SinkInput.hpp>
#include <GhafAudioControl/Backends/PulseAudio/Source.hpp>
#include <GhafAudioControl/Backends/PulseAudio/SourceOutput.hpp>

#include <GhafAudioControl/utils/Logger.hpp>

#include <pulse/error.h>
#include <pulse/glib-mainloop.h>
#include <pulse/introspect.h>
#include <pulse/subscribe.h>

#include <glibmm/main.h>

#include <format>

namespace ghaf::AudioControl::Backend::PulseAudio
{

namespace
{

template<class DeviceT, class InfoT, class IDeviceT>
void OnPulseDeviceInfo(const InfoT& info, bool isDefault, IAudioControlBackend::SignalMap<IDeviceT>& map, pa_context& context)
{
    const Index index = info.index;

    if (auto deviceIt = map.findByKey(index))
    {
        const IDeviceT& device = *deviceIt.value()->second;

        map.update(*deviceIt,
                   [&info, isDefault](IDeviceT& device)
                   {
                       dynamic_cast<DeviceT&>(device).update(info);

                       if (auto* defaultable = dynamic_cast<IAudioControlBackend::IDefaultable*>(&device))
                           defaultable->updateDefault(isDefault);

                       return true;
                   });

        Logger::debug("Updating... {}", device.toString());
    }
    else
    {
        if constexpr (std::is_base_of_v<IAudioControlBackend::IDefaultable, IDeviceT>)
            map.add(index, std::make_shared<DeviceT>(info, isDefault, context));
        else
            map.add(index, std::make_shared<DeviceT>(info, context));
    }
}

template<class DeviceT, class IndexT, class IDeviceT>
void DeletePulseDevice(IAudioControlBackend::SignalMap<IDeviceT>& map, IndexT index)
{
    if (auto deviceIt = map.findByKey(index))
    {
        Logger::debug("AudioControlBackend::DeletePulseDevice: delete device with id: {}", index);
        map.remove(*deviceIt,
                   [](IDeviceT& device)
                   {
                       dynamic_cast<DeviceT&>(device).markDeleted();
                       return true;
                   });
    }
    else
        Logger::error("AudioControlBackend::DeletePulseDevice: no device with id: {}", index);
}

std::string ToString(const pa_card_port_info& port)
{
    return std::format("Port. Name: {}, descript: {}, available: {}", port.name, port.description, port.available);
}

constexpr pa_subscription_mask GetSubscriptionMask(auto... masks)
{
    return static_cast<pa_subscription_mask>((masks | ...));
}

constexpr pa_subscription_mask SubscriptionMask = GetSubscriptionMask(pa_subscription_mask::PA_SUBSCRIPTION_MASK_SERVER,
                                                                      pa_subscription_mask::PA_SUBSCRIPTION_MASK_SINK,
                                                                      pa_subscription_mask::PA_SUBSCRIPTION_MASK_SINK_INPUT,
                                                                      pa_subscription_mask::PA_SUBSCRIPTION_MASK_SOURCE,
                                                                      pa_subscription_mask::PA_SUBSCRIPTION_MASK_SOURCE_OUTPUT,
                                                                      pa_subscription_mask::PA_SUBSCRIPTION_MASK_CARD);

[[nodiscard]] RaiiWrap<pa_glib_mainloop*> InitMainloop()
{
    return {[](pa_glib_mainloop*& mainloop)
            {
                if (mainloop = pa_glib_mainloop_new(Glib::MainContext::get_default().get()->gobj()); mainloop == nullptr)
                    throw std::runtime_error("pa_glib_mainloop_new() failed.");
            },
            [](pa_glib_mainloop*& mainloop)
            {
                pa_glib_mainloop_free(mainloop);
            }};
}

[[nodiscard]] RaiiWrap<pa_mainloop_api*> InitApi(pa_glib_mainloop& mainloop)
{
    return {[&mainloop](pa_mainloop_api*& api)
            {
                if (api = pa_glib_mainloop_get_api(&mainloop); api == nullptr)
                    throw std::runtime_error("pa_glib_mainloop_get_api() failed.");
            },
            RaiiWrap<pa_mainloop_api*>::Destructor()};
}

[[nodiscard]] RaiiWrap<pa_context*> InitContext(pa_mainloop_api& api, pa_context_notify_cb_t contextCallback, AudioControlBackend& self)
{
    const auto constructor = [&api, &contextCallback, &self](pa_context*& context)
    {
        const auto propListConstructor = [](pa_proplist*& proplist)
        {
            proplist = pa_proplist_new();
            std::ignore = pa_proplist_sets(proplist, PA_PROP_APPLICATION_NAME, "Ghaf Audio Control");
            std::ignore = pa_proplist_sets(proplist, PA_PROP_APPLICATION_ID, "org.ghaf.audiocontrol");
            std::ignore = pa_proplist_sets(proplist, PA_PROP_APPLICATION_ICON_NAME, "audio-card");
        };

        const auto propListDestructor = [](pa_proplist*& proplist)
        {
            pa_proplist_free(proplist);
        };

        const RaiiWrap<pa_proplist*> propList(propListConstructor, propListDestructor);

        if (context = pa_context_new_with_proplist(&api, "GhafAudioControl", propList); context == nullptr)
            throw std::runtime_error("pa_context_new_with_proplist() failed.");

        pa_context_set_state_callback(context, contextCallback, &self);

        if (const auto& server = self.getServerAddress(); pa_context_connect(context, server.empty() ? nullptr : server.c_str(), PA_CONTEXT_NOFAIL, nullptr) < 0)
            throw std::runtime_error(std::format("pa_context_connect() failed: {}", pa_strerror(pa_context_errno(context))));
    };

    const auto destructor = [](pa_context*& context)
    {
        pa_context_disconnect(context);
    };

    return {constructor, destructor};
}

} // namespace

AudioControlBackend::AudioControlBackend(std::string pulseAudioServerAddress)
    : m_serverAddress(std::move(pulseAudioServerAddress))
    , m_mainloop(InitMainloop())
    , m_mainloopApi(InitApi(*m_mainloop))
{
}

void AudioControlBackend::start()
{
    Logger::info("PulseAudio::AudioControlBackend: starting with server: {}", m_serverAddress);
    m_context = InitContext(*m_mainloopApi, contextStateCallback, *this);
}

void AudioControlBackend::stop()
{
    m_context.reset();
}

void AudioControlBackend::setDeviceVolume(IDevice::IntexT index, IDevice::Type type, Volume volume)
{
    const auto update = [index, volume](auto& map)
    {
        if (auto iterator = map.findByKey(index))
            map.update(*iterator,
                       [volume](auto& device)
                       {
                           device.setVolume(volume);
                           return true;
                       });
        else
            Logger::error("AudioControlBackend::setDeviceVolume: no such a device with id: {}", index);
    };

    switch (type)
    {
    case IAudioControlBackend::IDevice::Type::Sink:
        update(m_sinks);
        break;

    case IAudioControlBackend::IDevice::Type::Source:
        update(m_sources);
        break;

    case IAudioControlBackend::IDevice::Type::SinkInput:
        update(m_sinkInputs);
        break;

    case IAudioControlBackend::IDevice::Type::SourceOutput:
        update(m_sourceOutputs);
        break;
    }
}

void AudioControlBackend::setDeviceMute(IDevice::IntexT index, IDevice::Type type, bool mute)
{
    const auto update = [index, mute](auto& map)
    {
        if (auto iterator = map.findByKey(index))
            map.update(*iterator,
                       [mute](auto& device)
                       {
                           device.setMuted(mute);
                           return true;
                       });
        else
            Logger::error("AudioControlBackend::setDeviceMute: no such a device with id: {}", index);
    };

    switch (type)
    {
    case IAudioControlBackend::IDevice::Type::Sink:
        update(m_sinks);
        break;

    case IAudioControlBackend::IDevice::Type::Source:
        update(m_sources);
        break;

    case IAudioControlBackend::IDevice::Type::SinkInput:
        update(m_sinkInputs);
        break;

    case IAudioControlBackend::IDevice::Type::SourceOutput:
        update(m_sourceOutputs);
        break;
    }
}

void AudioControlBackend::makeDeviceDefault(IDevice::IntexT index, IDevice::Type type)
{
    const auto update = [index](auto& map)
    {
        if (auto iterator = map.findByKey(index))
            map.update(*iterator,
                       [](auto& device)
                       {
                           device.setDefault(true);
                           return true;
                       });
        else
            Logger::error("AudioControlBackend::makeDeviceDefault: no such a device with id: {}", index);
    };

    switch (type)
    {
    case IAudioControlBackend::IDevice::Type::Sink:
        update(m_sinks);
        break;

    case IAudioControlBackend::IDevice::Type::Source:
        update(m_sources);
        break;

    default:
        break;
    }
}

std::vector<IAudioControlBackend::IDevice::Ptr> AudioControlBackend::getAllDevices() const
{
    std::vector<IAudioControlBackend::IDevice::Ptr> result;

    const auto copyToResult = [&result](const auto& vec)
    {
        result.insert(result.begin(), vec.begin(), vec.end());
    };

    copyToResult(m_sinks.getAllValues());
    copyToResult(m_sources.getAllValues());
    copyToResult(m_sinkInputs.getAllValues());
    copyToResult(m_sourceOutputs.getAllValues());

    return result;
}

void AudioControlBackend::onSinkInfo(const pa_sink_info& info)
{
    OnPulseDeviceInfo<Sink>(info, m_defaultSinkName == info.name, m_sinks, *m_context->get());
}

void AudioControlBackend::deleteSink(Sinks::IndexT index)
{
    DeletePulseDevice<Sink>(m_sinks, index);
}

void AudioControlBackend::onSourceInfo(const pa_source_info& info)
{
    OnPulseDeviceInfo<Source>(info, m_defaultSourceName == info.name, m_sources, *m_context->get());
}

void AudioControlBackend::deleteSource(Sources::IndexT index)
{
    DeletePulseDevice<Source>(m_sources, index);
}

void AudioControlBackend::onSinkInputInfo(const pa_sink_input_info& info)
{
    OnPulseDeviceInfo<SinkInput>(info, false, m_sinkInputs, *m_context->get());
}

void AudioControlBackend::deleteSinkInput(SinkInputs::IndexT index)
{
    DeletePulseDevice<SinkInput>(m_sinkInputs, index);
}

void AudioControlBackend::onSourceOutputInfo(const pa_source_output_info& info)
{
    OnPulseDeviceInfo<SourceOutput>(info, false, m_sourceOutputs, *m_context->get());
}

void AudioControlBackend::deleteSourceOutput(SourceOutputs::IndexT index)
{
    DeletePulseDevice<SourceOutput>(m_sourceOutputs, index);
}

void AudioControlBackend::onServerInfo(const pa_server_info& info)
{
    const auto updateSinkFunction = [defaultName = m_defaultSinkName](ISink& sink)
    {
        const bool shouldBeDefault = sink.getName() == defaultName;

        if (sink.isDefault() == shouldBeDefault)
            return false;

        dynamic_cast<Sink&>(sink).updateDefault(shouldBeDefault);

        return true;
    };

    const auto updateSourceFunction = [defaultName = m_defaultSourceName](ISource& source)
    {
        const bool shouldBeDefault = source.getName() == defaultName;

        if (source.isDefault() == shouldBeDefault)
            return false;

        dynamic_cast<Source&>(source).updateDefault(shouldBeDefault);

        return true;
    };

    if (m_defaultSinkName != info.default_sink_name)
    {
        Logger::info("AudioControlBackend::onServerInfo: default sink set to: {}", info.default_sink_name);

        m_defaultSinkName = info.default_sink_name;
        // m_sinks.forEach(updateSinkFunction);
    }

    if (m_defaultSourceName != info.default_source_name)
    {
        Logger::info("AudioControlBackend::onServerInfo: default source set to: {}", info.default_source_name);

        m_defaultSourceName = info.default_source_name;
        // m_sources.forEach(updateSourceFunction);
    }
}

void AudioControlBackend::subscribeCallback(pa_context* context, pa_subscription_event_type_t type, uint32_t index, void* data)
{
    auto* self = static_cast<AudioControlBackend*>(data);
    const bool needRemove = (type & pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_TYPE_MASK) == pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_REMOVE;
    const auto eventType = static_cast<pa_subscription_event_type>(type & pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_FACILITY_MASK);

    switch (eventType)
    {
    case pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_SERVER:
        ExecutePulseFunc(pa_context_get_server_info, context, serverInfoCallback, data);
        break;

    case pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_SINK:
        if (needRemove)
            self->deleteSink(index);
        else
            ExecutePulseFunc(pa_context_get_sink_info_by_index, context, index, sinkInfoCallback, data);

        break;

    case pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_SINK_INPUT:
        if (needRemove)
            self->deleteSinkInput(index);
        else
            ExecutePulseFunc(pa_context_get_sink_input_info, context, index, sinkInputInfoCallback, data);

        break;

    case pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_SOURCE:
        if (needRemove)
            self->deleteSource(index);
        else
            ExecutePulseFunc(pa_context_get_source_info_by_index, context, index, sourceInfoCallback, data);

        break;

    case pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_SOURCE_OUTPUT:
        if (needRemove)
            self->deleteSourceOutput(index);
        else
            ExecutePulseFunc(pa_context_get_source_output_info, context, index, sourceOutputInfoCallback, data);

        break;

    case pa_subscription_event_type::PA_SUBSCRIPTION_EVENT_CARD:
        ExecutePulseFunc(pa_context_get_card_info_by_index, context, index, cardInfoCallback, data);
        break;

    default:
        Logger::error("subscribeCallback: unknown eventType: {0}", static_cast<int>(eventType));
        break;
    };
}

void AudioControlBackend::contextStateCallback(pa_context* context, void* data)
{
    auto* self = static_cast<AudioControlBackend*>(data);

    switch (const auto state = pa_context_get_state(context))
    {
    case pa_context_state_t::PA_CONTEXT_TERMINATED:
        self->m_onError("pa_context_state_t::PA_CONTEXT_TERMINATED");
        break;

    case pa_context_state_t::PA_CONTEXT_READY:
        ExecutePulseFunc(pa_context_get_server_info, context, serverInfoCallback, data);
        pa_context_set_subscribe_callback(context, subscribeCallback, data);
        ExecutePulseFunc(pa_context_subscribe, context, SubscriptionMask, nullptr, nullptr);
        break;

    case pa_context_state_t::PA_CONTEXT_FAILED:
        self->m_onError(std::format("Connection to the server '{}' has failed", self->getServerAddress()));
        break;

    case pa_context_state_t::PA_CONTEXT_CONNECTING:
    case pa_context_state_t::PA_CONTEXT_AUTHORIZING:
    case pa_context_state_t::PA_CONTEXT_SETTING_NAME:
        break;

    default:
        Logger::error("contextStateCb: unknown state: {0}", static_cast<int>(state));
        break;
    }
}

void AudioControlBackend::sinkInfoCallback(pa_context* context, const pa_sink_info* info, int eol, void* data)
{
    if (!PulseCallbackCheck(context, eol, __FUNCTION__) || info == nullptr)
        return;

    static_cast<AudioControlBackend*>(data)->onSinkInfo(*info);
}

void AudioControlBackend::sourceInfoCallback(pa_context* context, const pa_source_info* info, int eol, void* data)
{
    if (!PulseCallbackCheck(context, eol, __FUNCTION__) || info == nullptr)
        return;

    static_cast<AudioControlBackend*>(data)->onSourceInfo(*info);
}

void AudioControlBackend::sinkInputInfoCallback(pa_context* context, const pa_sink_input_info* info, int eol, void* data)
{
    if (!PulseCallbackCheck(context, eol, __FUNCTION__) || info == nullptr)
        return;

    static_cast<AudioControlBackend*>(data)->onSinkInputInfo(*info);
}

void AudioControlBackend::sourceOutputInfoCallback(pa_context* context, const pa_source_output_info* info, int eol, void* data)
{
    if (!PulseCallbackCheck(context, eol, __FUNCTION__) || info == nullptr)
        return;

    static_cast<AudioControlBackend*>(data)->onSourceOutputInfo(*info);
}

void AudioControlBackend::serverInfoCallback(pa_context* context, const pa_server_info* info, void* data)
{
    if (info == nullptr)
        return;

    static_cast<AudioControlBackend*>(data)->onServerInfo(*info);

    ExecutePulseFunc(pa_context_get_sink_info_list, context, sinkInfoCallback, data);
    ExecutePulseFunc(pa_context_get_source_info_list, context, sourceInfoCallback, data);
    ExecutePulseFunc(pa_context_get_sink_input_info_list, context, sinkInputInfoCallback, data);
    ExecutePulseFunc(pa_context_get_source_output_info_list, context, sourceOutputInfoCallback, data);
    ExecutePulseFunc(pa_context_get_card_info_list, context, cardInfoCallback, data);
}

void AudioControlBackend::cardInfoCallback(pa_context* context, const pa_card_info* info, int eol, void* data)
{
    if (!PulseCallbackCheck(context, eol, __FUNCTION__))
        return;

    if (info == nullptr)
        return;

    Logger::debug("###############################################");
    Logger::debug("Card. index: {}, name: {}", info->index, info->name);

    for (size_t i = 0; i < info->n_ports; ++i)
        Logger::info(ToString(*info->ports[i]));

    Logger::debug("###############################################\n");

    auto* self = static_cast<AudioControlBackend*>(data);

    const auto sinkPredicate = [&info](const ISink& sink)
    {
        return dynamic_cast<const Sink&>(sink).getCardIndex() == info->index;
    };

    for (auto it : self->m_sinks.findByPredicate(sinkPredicate))
        self->m_sinks.update(it,
                             [&info](ISink& sink)
                             {
                                 dynamic_cast<Sink&>(sink).update(*info);
                                 return true;
                             });

    const auto sourcePredicate = [&info](const ISource& source)
    {
        return dynamic_cast<const Source&>(source).getCardIndex() == info->index;
    };

    for (auto it : self->m_sources.findByPredicate(sourcePredicate))
        self->m_sources.update(it,
                               [&info](ISource& source)
                               {
                                   dynamic_cast<Source&>(source).update(*info);
                                   return true;
                               });
}

} // namespace ghaf::AudioControl::Backend::PulseAudio
