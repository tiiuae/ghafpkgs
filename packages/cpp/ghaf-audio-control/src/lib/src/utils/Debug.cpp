/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include <GhafAudioControl/utils/Debug.hpp>

#include <atomic>
#include <cassert>

#include <glib.h>

namespace ghaf::AudioControl
{

std::atomic_bool UiThreadStarted = false;

void MarkUiTreadStarted()
{
    UiThreadStarted = true;
}

void CheckUiThread()
{
    if (UiThreadStarted)
        assert(g_main_context_is_owner(nullptr) && "Accessing widgets out of the UI thread!");
}

} // namespace ghaf::AudioControl
