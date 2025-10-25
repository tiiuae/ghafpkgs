/*
 * SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
 * SPDX-License-Identifier: Apache-2.0
 */

// set some commentary
#include <GhafAudioControl/utils/ConnectionContainer.hpp>

namespace ghaf::AudioControl
{

ConnectionContainer::ConnectionContainer(std::initializer_list<sigc::connection> connections)
    : m_connections{connections}
{
}

ConnectionContainer::~ConnectionContainer()
{
    clear();
}

ScopeExit ConnectionContainer::blockGuarded()
{
    block();

    return {[this]() { unblock(); }};
}

void ConnectionContainer::block()
{
    forEach(&sigc::connection::block, true);
}

void ConnectionContainer::unblock()
{
    forEach(&sigc::connection::unblock);
}

void ConnectionContainer::clear()
{
    block();
    forEach(&sigc::connection::disconnect);

    m_connections.clear();
}

} // namespace ghaf::AudioControl
