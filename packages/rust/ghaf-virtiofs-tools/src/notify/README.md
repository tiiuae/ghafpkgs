<!--
SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# virtiofs-notify

Guest notification receiver for virtiofs file change events.

## Overview

virtiofs-notify runs inside guest VMs and listens for channel notifications from the host's virtiofs-gate daemon. When notified, it triggers a file browser refresh by toggling a hidden file, causing inotify events that file managers detect.

## How It Works

### Notification Flow

1. Host's virtiofs-gate exports a file to guest shares
2. virtiofs-gate sends channel name to guest via vsock
3. virtiofs-notify receives channel name
4. Looks up mapped directory for that channel
5. Toggles `.virtiofs-refresh` file to trigger inotify event
6. File browser detects change and refreshes view

### Why Toggle a File?

Some file browsers don't refresh on:

- `touch` (mtime update only)
- Metadata changes
- Events they consider "unimportant"

By creating or deleting an actual file, we generate `IN_CREATE` or `IN_DELETE` events that all file browsers respond to. The file is hidden (dot prefix) to avoid clutter.

### Protocol

Simple line-based protocol over vsock:

```text
channel_name\n
```

The channel name maps to a local directory path via `--map` arguments.

## CLI Usage

```bash
# Map channels to local mount points
virtiofs-notify \
  --map documents=/mnt/share/documents \
  --map media=/mnt/share/media

# Custom vsock port
virtiofs-notify --port 3401 --map documents=/mnt/share/documents

# Enable debug logging
virtiofs-notify --map documents=/mnt/share/documents --debug
```

### Options

| Option | Default | Description |
| ------ | ------- | ----------- |
| `--port`, `-p` | `3401` | vsock port to listen on |
| `--map`, `-m` | required | Channel to path mapping (channel=path) |
| `--debug`, `-d` | `false` | Enable debug logging |

Multiple `--map` arguments can be specified for different channels.

## Configuration Example

If virtiofs-gate has a channel named "documents" with `notify.guests` including this VM's CID:

```bash
# Guest VM startup
virtiofs-notify --map documents=/mnt/virtiofs/documents
```

When files are exported to the documents channel, the file browser showing `/mnt/virtiofs/documents` will refresh automatically.

## Failure Handling

| Failure | Behavior |
| ------- | -------- |
| Mapped path doesn't exist at startup | Daemon refuses to start |
| Unknown channel received | Ignored silently |
| Trigger file operation fails | Warning logged, continues |
| vsock connection drops | Connection closed, listener continues |

## Integration with virtiofs-gate

The host's virtiofs-gate config specifies which guests to notify:

```json
{
  "documents": {
    "basePath": "/var/lib/virtiofs/documents",
    "notify": {
      "guests": [10, 11],
      "port": 3401
    }
  }
}
```

Each guest VM listed in `guests` should run virtiofs-notify with a matching `--map` for the channel.

## Requirements

- **vsock**: vhost-vsock device available in guest
- **Permissions**: Write access to mapped directories (to create trigger file)
- **virtiofs-gate**: Running on host with notify config for this guest's CID
