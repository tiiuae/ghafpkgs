# vinotify

Virtual machine file system notification service using inotify.

This package provides functionality for monitoring file system changes and sending notifications over VSOCK connections, enabling communication between host and guest VMs.

## Features

- File system monitoring using Linux inotify
- VSOCK socket communication for VM integration
- Real-time file change notifications
- Command-line interface
- Configurable monitoring paths and connection parameters

## Usage

```bash
vinotify --path <directory> --cid <context_id> --port <port>
```

## Dependencies

- inotify_simple: Linux inotify interface library

## License

Apache-2.0
