# ghaf-virtiofs-tools

Tools for secure cross-VM file sharing over virtiofs with integrated virus scanning.

## Components

| Binary | Location | Description |
|--------|----------|-------------|
| `virtiofs-gate` | Host | Gateway daemon - scans files and propagates to VMs |
| `clamd-vclient` | Guest | On-write scanner - watches directories, scans via vsock |
| `clamd-vproxy` | Host | ClamAV proxy - filters commands, forwards to clamd |
| `virtiofs-notify` | Guest | Notification receiver - triggers file browser refresh |

## Use Cases

### Shared Directories with Scanning (virtiofs-gate)

Secure file sharing between VMs with automatic virus scanning.

```text
Guest VM A writes file.txt
         |
         | (virtiofs)
         v
+------------------HOST------------------+
|                                        |
|  share/vm-a/file.txt                   |
|         |                              |
|         | (clone to staging)           |
|         v                              |
|  staging/file.txt ---> clamd (scan)    |
|         |                              |
|         | clean?                       |
|         v                              |
|  +------+-------+                      |
|  |              |                      |
|  v              v                      |
|  share/vm-b/  export/                  |
|  file.txt     file.txt                 |
|  (reflink)    (reflink)                |
|                                        |
+----------------------------------------+
         |
         | (virtiofs)
         v
Guest VM B sees file.txt
```

Infected files are quarantined or deleted instead of propagated.

See [src/gate/README.md](src/gate/README.md)

### On-Write Scanning (clamd-vclient + clamd-vproxy)

Scan files written inside a guest VM using host's ClamAV.

```text
+------------GUEST VM-----------+
|                               |
|  App writes file.txt          |
|         |                     |
|         v                     |
|  ~/Downloads/file.txt         |
|         |                     |
|  clamd-vclient detects write  |
|         |                     |
|  read file contents           |
|         |                     |
+---------|--(vsock)------------+
          v
+---------HOST------------------+
|                               |
|  clamd-vproxy receives stream |
|         |                     |
|         | (filter: INSTREAM   |
|         |  only, block SCAN)  |
|         v                     |
|  clamd scans stream           |
|         |                     |
|  returns: clean / infected    |
|                               |
+---------|--(vsock)------------+
          v
+---------GUEST VM--------------+
|                               |
|  clamd-vclient receives result|
|         |                     |
|         v                     |
|  infected? --> delete/quarantine
|                               |
+-------------------------------+
```

See [src/vclient/README.md](src/vclient/README.md) and [src/vproxy/README.md](src/vproxy/README.md)

### File Browser Refresh (virtiofs-notify)

Notify guests when new files appear in shared directories.

```text
+-----------HOST----------------+
|                               |
|  virtiofs-gate exports file   |
|  to share/vm-b/               |
|         |                     |
|  send "channel-name"          |
|         |                     |
+---------|--(vsock)------------+
          v
+---------GUEST VM--------------+
|                               |
|  virtiofs-notify receives     |
|  "channel-name"               |
|         |                     |
|  lookup: channel -> /mnt/share|
|         |                     |
|  toggle .virtiofs-refresh     |
|  (create or delete)           |
|         |                     |
|         v                     |
|  inotify event fires          |
|         |                     |
|         v                     |
|  file browser refreshes view  |
|                               |
+-------------------------------+
```

See [src/notify/README.md](src/notify/README.md)

## Building

```bash
# Build with nix
nix build .#ghaf-virtiofs-tools

# Build with cargo
cargo build --release
```

## Requirements

- **Filesystem**: btrfs or XFS with reflink support (for virtiofs-gate)
- **ClamAV**: clamd daemon for virus scanning
- **vsock**: vhost-vsock for guest communication
- **virtiofs**: For shared directory mounts

## License

Apache-2.0
