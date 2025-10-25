<!--
Copyright 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# vsockproxy

A VM Sockets proxy for guest-to-guest communication in virtualized environments.

## Overview

VM Sockets (vsock) is a communication mechanism between guest virtual machines and the host. The `vsockproxy` tool enables guest-to-guest communication by acting as a bridge that listens for incoming connections on the host, connects to target guest virtual machines, and forwards data bidirectionally.

This tool is particularly useful in virtualized environments where direct guest-to-guest communication needs to be controlled and routed through the host system.

## Features

- **Guest-to-Guest Communication**: Enables indirect communication between guest VMs via the host
- **Access Control**: Configurable CID (Context ID) filtering to allow only specific guests
- **Bidirectional Forwarding**: Full-duplex data forwarding between connections
- **High Performance**: Efficient event-driven I/O using epoll for scalability
- **Statistics Reporting**: Built-in connection and data transfer statistics
- **Signal Handling**: Graceful shutdown on SIGTERM and SIGINT

## Usage

### Basic Syntax

```bash
vsockproxy <local_port> <remote_cid> <remote_port> <allowed_cid>
```

### Parameters

- **`local_port`** - Port number on host where vsockproxy listens for incoming TCP connections from guest VMs
- **`remote_cid`** - Context ID (CID) of the target guest VM where data will be forwarded
- **`remote_port`** - Port number on the target guest VM's vsock interface
- **`allowed_cid`** - CID of the guest VM allowed to connect (use `0` to allow any guest); connections from other CIDs will be denied

### Examples

```bash
# Allow any guest (CID 0) to connect to port 8080, forward to guest 3 on port 9090
vsockproxy 8080 3 9090 0

# Only allow guest 2 to connect to port 8080, forward to guest 4 on port 9090
vsockproxy 8080 4 9090 2

# Create a bridge between guest 5 (allowed) and guest 7 (target) via host port 3000
vsockproxy 3000 7 22 5
```

### Common Use Cases

1. **SSH Tunneling**: Forward SSH connections between guests
   ```bash
   vsockproxy 2222 3 22 2  # Guest 2 can SSH to guest 3 via host:2222
   ```

2. **Database Access**: Allow specific guests to access database on another guest
   ```bash
   vsockproxy 5432 4 5432 6  # Guest 6 can access PostgreSQL on guest 4
   ```

3. **Web Service Proxy**: Forward HTTP traffic between guests
   ```bash
   vsockproxy 8080 5 80 0  # Any guest can access web service on guest 5
   ```

## Architecture

The vsockproxy operates as a TCP-to-vsock bridge:

1. **TCP Listener**: Binds to specified port on host, accepts TCP connections from guests
2. **CID Validation**: Checks if connecting guest's CID is allowed
3. **vsock Connection**: Establishes connection to target guest's vsock
4. **Data Forwarding**: Bidirectional forwarding using efficient epoll-based I/O
5. **Statistics**: Periodic reporting of active connections and throughput

```
[Guest VM A] --TCP--> [Host:vsockproxy] --vsock--> [Guest VM B]
     CID=2              Port 8080                    CID=3:Port=9090
```

## Building

### Using Nix (Recommended)

```bash
# Build the package
nix build .#vsockproxy

# Run directly
nix run .#vsockproxy -- 8080 3 9090 0
```

### Manual Build with Meson

```bash
# Initialize build directory
meson setup build
cd build

# Compile
meson compile

# Install (optional)
meson install
```

## Integration with Ghaf

This tool is designed to work within the Ghaf virtualization framework:

- **VM Communication**: Enables controlled inter-VM communication
- **Security**: Provides access control through CID filtering
- **Performance**: Optimized for high-throughput VM networking
- **Monitoring**: Built-in statistics for network analysis

## Security Considerations

- **CID Filtering**: Always specify appropriate `allowed_cid` values to restrict access
- **Network Isolation**: Use in conjunction with proper network policies
- **Host Security**: Ensure host firewall rules complement vsockproxy access controls
- **Monitoring**: Regularly review connection statistics for anomalies

## Troubleshooting

### Common Issues

1. **Connection Refused**
   - Verify target guest VM is running and has vsock enabled
   - Check that `remote_port` is available on target guest

2. **Permission Denied**
   - Ensure connecting guest's CID matches `allowed_cid` (or use `0` for any)
   - Verify vsockproxy has necessary permissions

3. **High CPU Usage**
   - Monitor connection count and data throughput
   - Consider multiple vsockproxy instances for load distribution

### Debug Information

The tool provides runtime statistics including:
- Active connection count
- Data transfer rates
- Connection establishment/termination events

## Dependencies

- **Linux Kernel**: VM sockets support (CONFIG_VSOCKETS)
- **libc**: Standard C library
- **Build Tools**: Meson, Ninja, GCC

## License

Copyright 2022-2026 TII (SSRC) and the Ghaf contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

## Authors

- TII (SSRC) and the Ghaf contributors

## Contributing

This project is part of the Ghaf operating system. For contributing guidelines, please refer to the main Ghaf project documentation.
