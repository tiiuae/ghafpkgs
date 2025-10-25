# GPS WebSocket Server

A GPS endpoint server that exposes GPS data over WebSocket connections.

## Overview

This package provides a WebSocket server that reads GPS data from `gpspipe` and broadcasts it to connected WebSocket clients. It's designed to work as part of the Ghaf operating system for GPS data sharing between components.

## Features

- **Real-time GPS Data**: Continuously reads GPS data from `gpspipe`
- **WebSocket Broadcasting**: Broadcasts GPS TPV (Time-Position-Velocity) data to all connected clients
- **Signal Handling**: Graceful shutdown on SIGTERM and SIGINT
- **Async/Await Support**: Built using modern Python asyncio patterns
- **Multiple Clients**: Supports multiple concurrent WebSocket connections

## Usage

Start the GPS WebSocket server:

```bash
gpswebsock
```

The server will:
1. Start listening on `localhost:8000` for WebSocket connections
2. Launch `gpspipe -w` to read GPS data
3. Filter for TPV (Time-Position-Velocity) messages
4. Broadcast GPS data to all connected WebSocket clients

## WebSocket Protocol

- **Endpoint**: `ws://localhost:8000`
- **Data Format**: JSON messages containing GPS TPV data from gpsd
- **Connection**: Clients connect and receive continuous GPS updates

Example GPS message format:
```json
{
  "class": "TPV",
  "device": "/dev/ttyUSB0",
  "time": "2023-01-01T00:00:00.000Z",
  "lat": 60.1699,
  "lon": 24.9384,
  "alt": 10.0,
  "speed": 0.0,
  "track": 0.0
}
```

## Dependencies

- **Python**: >=3.11
- **websockets**: >=12.0 (for WebSocket server functionality)
- **gpsd**: System requirement for GPS data (accessed via `gpspipe`)

## Architecture

The application consists of several async components:

- `GpsProcessState`: Manages shared state between GPS reader and WebSocket handlers
- `read_continuous_gps()`: Reads GPS data from gpspipe subprocess
- `handler()`: Manages individual WebSocket client connections
- `wait_connection()`: WebSocket server listening for new connections
- `signal_handler()`: Graceful shutdown handling

## System Integration

This package is designed to work with:
- **gpsd**: GPS daemon that provides GPS data via `gpspipe`
- **Ghaf OS**: Part of the Ghaf secure operating system ecosystem

## Development

### Building from Source

The package uses Hatchling as the build backend and supports uv for development:

```bash
# Using uv (recommended for development)
uv sync
uv run gpswebsock

# Using pip
pip install -e .
```

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
