/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/Backends/PulseAudio/Helpers.hpp>

#include <pulse/error.h>

#include <format>

namespace ghaf::AudioControl::Backend::PulseAudio
{

bool PulseCallbackCheck(const pa_context* context, int eol, std::string_view callbackName)
{
    if (context == nullptr)
    {
        Logger::error(std::format("pulseCallbackCheck: callback: {} context == nullptr", callbackName));
        return false;
    }

    if (eol == 0)
        return true;

    const auto error = pa_context_errno(context);

    if (error == pa_error_code::PA_ERR_NOENTITY || error == pa_error_code::PA_OK)
        return true;

    Logger::error(std::format("pulseCallbackCheck: callback: {} failed with error: {}", callbackName, pa_strerror(error)));

    return false;
}

} // namespace ghaf::AudioControl::Backend::PulseAudio
