# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

# Enhanced Cross-Bus GDBus Proxy

## Overview

This C++ program implements an **Enhanced Cross-Bus GDBus Proxy** that facilitates communication between two distinct D-Bus buses (e.g., system and session). It dynamically introspects a source service and exposes its interface on a target bus, forwarding method calls, signals, and property changes between the two.

---

## Features

- Connects to two different D-Bus buses (source and target).
- Fetches introspection data from a source service.
- Registers and exposes the source interface on the target bus.
- Forwards method calls from the target bus to the source bus.
- Forwards signals from the source bus to the target bus.
- Synchronizes properties between buses.
- Verbose logging for debugging and monitoring.

---

## Dependencies

- **GLib**
- **GIO (GDBus)**
- **pkg-config**

Ensure these libraries are installed on your system before building the program.

---

## Building

Use `g++` with `pkg-config` to compile:

```bash
mkdir build
cd build
cmake ..
make
```

---

## Running

```bash
./dbus-proxy --source-bus-name org.freedesktop.NetworkManager \
             --source-object-path /org/freedesktop/NetworkManager \
             --proxy-bus-name org.example.Proxy \
             --source-bus-type system \
             --target-bus-type session \
             --verbose
```

---

## Configuration Options

| Option                  | Description                                 |
|-------------------------|---------------------------------------------|
| `--source-bus-name`     | D-Bus name of the source service.           |
| `--source-object-path`  | Object path of the source service.          |
| `--proxy-bus-name`      | Bus name to expose on the target bus.       |
| `--source-bus-type`     | Type of source bus: `system` or `session`.  |
| `--target-bus-type`     | Type of target bus: `system` or `session`.  |
| `--verbose`             | Enable verbose logging.                     |
| `--help`                | Show usage information.                     |


---


## Example Use Case

You want to expose the `NetworkManager` service from the system bus to the session bus for testing or sandboxing purposes. This proxy will mirror the interface and forward all interactions seamlessly.
