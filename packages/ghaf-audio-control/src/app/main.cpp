/*
 * Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

#include "App.hpp"

#include <GhafAudioControl/utils/Check.hpp>

using namespace ghaf::AudioControl;

int main(int argc, char** argv)
{
    pthread_setname_np(pthread_self(), "main");

    try
    {
        App app(argc, argv);
        return app.start();
    }
    catch (const Glib::Error& ex)
    {
        Logger::error("Error: {}", ex.what().c_str());
        return 1;
    }
    catch (const std::exception& e)
    {
        Logger::error("Exception: {}", e.what());
        return 1;
    }
}
