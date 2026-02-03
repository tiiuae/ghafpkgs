# clamd-vclient

Guest daemon for on-write virus scanning via ClamAV.

## Overview

clamd-vclient monitors directories inside a guest VM and scans modified files using ClamAV. It can connect to the host's clamd-vproxy over vsock, or use a local ClamAV socket directly. Infected files are deleted, quarantined, or logged based on configuration.

## How It Works

### Scan Flow

1. inotify detects `IN_CLOSE_WRITE` or `IN_MOVED_TO` event in watched directory
2. File path is queued for scanning
3. File content is read and sent via INSTREAM protocol
4. Response is parsed for scan result
5. If infected: delete, quarantine, or log based on `--action`
6. User notification sent to socket

### Scanning Backends

Two scanning modes are supported:

- **vsock** (default): Connects to clamd-vproxy on the host via virtio-vsock. File contents are streamed using ClamAV's INSTREAM protocol.

- **local socket** (`--socket`): Connects directly to a local clamd Unix socket. Useful when ClamAV runs inside the guest VM.

The client could also connect to a scanning daemon in a different guest, if either a vsock host proxy or socket forwarding is enabled.

### INSTREAM Protocol

The client sends:
1. `zINSTREAM\0` command
2. Chunk: 4-byte big-endian size + data
3. End marker: 4 zero bytes

The server responds with `stream: OK\n` or `stream: <virus> FOUND\n`.

## CLI Usage

```bash
# Watch directories, scan via host proxy (default)
clamd-vclient --watch /home/user/Downloads --watch /home/user/Documents

# Exclude directories from watching
clamd-vclient --watch /home/user --exclude /home/user/.cache

# Use local ClamAV socket instead of vsock
clamd-vclient --watch /home/user/Downloads --socket

# Custom vsock connection
clamd-vclient --watch /home/user/Downloads --cid 2 --port 3400

# Quarantine infected files instead of deleting
clamd-vclient --watch /home/user/Downloads --action quarantine --quarantine-dir /var/quarantine

# Just log infections, don't delete
clamd-vclient --watch /home/user/Downloads --action log

# Enable debug logging
clamd-vclient --watch /home/user/Downloads --debug
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--watch`, `-w` | required | Directories to monitor (can specify multiple) |
| `--exclude`, `-e` | none | Directories to exclude from recursive watching |
| `--cid`, `-c` | `2` | vsock CID (2 = host) |
| `--port`, `-p` | `3400` | vsock port for clamd-vproxy |
| `--socket`, `-s` | `false` | Use local ClamAV socket instead of vsock |
| `--action`, `-a` | `delete` | Action on infected: `log`, `delete`, `quarantine` |
| `--quarantine-dir` | none | Directory for quarantined files (required if action=quarantine) |
| `--notify-socket` | `/run/clamav/notify.sock` | Socket for user notifications |
| `--debug`, `-d` | `false` | Enable debug logging |

## Failure Handling

| Failure | Behavior |
|---------|----------|
| Proxy unavailable at startup | Daemon refuses to start |
| vsock connection fails during scan | Error logged, file skipped |
| File disappeared before scan | Skipped silently |
| Scan queue full | File dropped with warning |
| Quarantine fails | Error logged, file not deleted |
| Notification socket unavailable | Warning logged, continues |

## Use Cases

### Standalone Guest Scanning

For VMs with their own ClamAV installation:

```bash
clamd-vclient --watch /home/user/Downloads --socket
```

### Host-Proxied Scanning

For VMs without local ClamAV, using host's scanner via vsock:

```bash
clamd-vclient --watch /home/user/Downloads
```

This requires clamd-vproxy running on the host.

### Download Folder Protection

Monitor browser download directories for malware:

```bash
clamd-vclient \
  --watch /home/user/Downloads \
  --action delete \
  --notify-socket /run/user/1000/clamav-notify.sock
```

## Requirements

- **vsock mode**: clamd-vproxy running on host, vsock device available
- **socket mode**: clamd running locally with accessible socket
- **Permissions**: Read access to watched directories, write access for delete/quarantine
