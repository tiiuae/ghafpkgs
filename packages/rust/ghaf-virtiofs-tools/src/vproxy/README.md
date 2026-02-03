# clamd-vproxy

ClamAV vsock proxy with command filtering.

## Overview

clamd-vproxy accepts connections from guest VMs over virtio-vsock and forwards allowed ClamAV commands to the local clamd daemon. It filters commands to prevent guests from executing dangerous operations like scanning host files or shutting down the scanner.

## How It Works

### Command Filtering

Only these ClamAV commands are allowed:

| Command | Purpose |
|---------|---------|
| `INSTREAM` | Stream file contents for scanning |
| `PING` | Check if scanner is alive |
| `VERSION` | Get scanner version |

All other commands are blocked:

| Command | Risk |
|---------|------|
| `SCAN`, `CONTSCAN`, `MULTISCAN` | Could scan host filesystem |
| `SHUTDOWN` | Would kill clamd for all users |
| `RELOAD` | DoS via forced signature reload |

### Connection Flow

1. Guest connects via vsock
2. Proxy reads and parses command
3. If allowed: forward to clamd, relay response
4. If blocked: return error, close connection

### Rate Limiting

- **Max connections**: Configurable limit on concurrent connections (default: 10)
- **Stream size limit**: Maximum total INSTREAM size (default: 100MB)
- **Chunk size limit**: Maximum single chunk size (default: 25MB)
- **Timeouts**: Command, read, and total stream timeouts

## CLI Usage

```bash
# Start with defaults (CID 2, port 3400)
clamd-vproxy

# Custom vsock binding
clamd-vproxy --cid 2 --port 3400

# Custom clamd socket path
clamd-vproxy --clamd /var/run/clamav/clamd.sock

# Increase connection limit
clamd-vproxy --max-connections 20

# Adjust stream limits
clamd-vproxy --max-stream-size 209715200 --max-chunk-size 52428800

# Enable debug logging
clamd-vproxy --debug
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--cid`, `-c` | `2` | vsock CID to bind (2 = host) |
| `--port`, `-p` | `3400` | vsock port to listen on |
| `--clamd`, `-C` | `/run/clamav/clamd.ctl` | ClamAV daemon socket path |
| `--max-connections` | `10` | Max concurrent connections |
| `--max-stream-size` | `104857600` | Max total INSTREAM size (bytes) |
| `--max-chunk-size` | `26214400` | Max single chunk size (bytes) |
| `--command-timeout-secs` | `30` | Command read timeout |
| `--read-timeout-secs` | `60` | Per-read timeout |
| `--stream-timeout-secs` | `120` | Total INSTREAM timeout |
| `--debug`, `-d` | `false` | Enable debug logging |

## Failure Handling

| Failure | Behavior |
|---------|----------|
| clamd socket missing at startup | Proxy refuses to start |
| clamd connection fails during request | Error returned to client |
| Command not allowed | Error returned, connection closed |
| Stream size exceeded | Error returned, connection closed |
| Timeout exceeded | Error returned, connection closed |
| Max connections reached | New connections rejected |

## Security Considerations

### Why Filter Commands?

Without filtering, a compromised guest VM could:
- Use `SCAN /etc/shadow` to probe host files
- Use `SHUTDOWN` to disable scanning for all VMs
- Use `RELOAD` repeatedly to cause DoS

### INSTREAM Safety

The `INSTREAM` command is "safe" because:
- File contents are sent by the client, not read from host
- clamd only scans the streamed data
- No host filesystem access occurs

However, ClamAV vulnerabilities could be exploited.

### Limits Alignment

Configure limits to match or be below clamd settings:
- `max-connections` should not exceed clamd's `MaxThreads`
- `max-stream-size` should match clamd's `StreamMaxLength`
- Timeouts should be below clamd's `ReadTimeout`

## Requirements

- **clamd**: ClamAV daemon running with accessible Unix socket
- **vsock**: vhost-vsock kernel module loaded
- **Permissions**: Read/write access to clamd socket