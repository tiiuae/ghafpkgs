<!--
SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# ghaf-virtiofs-tools Test Plan

Comprehensive test plan for validating functionality, performance, and security.

## Test ID Numbering Convention

Test IDs are sequential within each major section, with space reserved at the end
for future expansion:

| Section | Prefix | Current Range | Reserved |
| --------- | -------- | --------------- | ---------- |
| 1. Functionality | F-, D-, S-, V-, VS-, R- | Per subsection | Per subsection |
| 2. Performance | P- | P-001 to P-018 | P-019+ |
| 3. Security | SEC- | SEC-001 to SEC-027 | SEC-028+ |

When adding new tests, append to the end of the relevant subsection.

## Test Environment Setup

### Required Channel Configuration

Tests require access to a virtiofs-gate channel with the following configurations:

| Channel Type | Mount | Tests | Configuration |
| -------------- | ------- | ------- | --------------- |
| RW Share | `share/<vm>/` | Most tests | Default producer mount |
| RO Export | `export/` | R-001 to R-003 | Consumer read-only mount |
| WO/Diode | `share/<vm>/` | D-001 to D-005 | `diodeProducers` configured |
| Scan Enabled | Any | S-001, S-002, V-* | `scanning.enabled = true` |
| Quarantine | Any | S-003 | `scanning.infectedAction = "quarantine"` |

For multi-producer tests (F-007, security tests), at least two producer VMs are required.
vsock tests (V-*, P-010 to P-012, SEC-021 to SEC-027) require `clamd-vproxy` on host.

### Directory Structure

```text
/persistent-storage/<channel>/
  share/
    <vm-a>/          # RW producer mount (virtiofsd -> vm-a)
    <vm-b>/          # RW producer mount (virtiofsd -> vm-b)
    <diode-vm>/      # WO diode producer (if diodeProducers configured)
  export/            # RO consumer mount (virtiofsd -> consumer-vm)
  staging/           # Gate daemon workspace (internal)
  quarantine/        # Infected files (if scanning.infectedAction = "quarantine")
```

### Required Services

- `virtiofs-gate` daemon on host/persistent-storage
- `clamd` (ClamAV daemon) on host
- `clamd-vproxy` for guest scanning
- `vinotify` on guest VMs (optional, for notification tests)

---

## 1. Basic Functionality Tests

### 1.1 virtiofs-gate Core Operations

| Test ID | Test Name | Description | Steps | Expected |
| --------- | ----------- | ------------- | ------- | ---------- |
| F-001 | File Create Sync | New file syncs to other producers | Create file in producer share | File appears in other producers |
| F-002 | File Modify Sync | Modified file syncs | Modify existing file | Updated content in other producers |
| F-003 | File Delete Sync | Deleted file removed | Delete file from producer | File removed from other producers |
| F-004 | File Rename Sync | Renamed file syncs | Rename file in producer | File renamed in other producers |
| F-005 | Directory Create | New directory watched | Create subdirectory | Directory monitored, files within sync |
| F-006 | Directory Delete | Directory removed | Delete directory tree | Removed from other producers |
| F-007 | Multi-Producer Propagation | File propagates to all producers | Create file in producer A | Appears in all other producers |
| F-008 | Empty File | Empty files propagate | Create 0-byte file | Syncs to other producers |
| F-009 | Debounce Consolidation | Rapid writes consolidated | Write to file 10 times in 100ms | Single scan after debounce |
| F-010 | Ignore File Pattern | Temp files ignored | Create `file.crdownload` | File not synced |
| F-011 | Ignore Path Pattern | Trash ignored | Create file in `.Trash-1000/` | File not synced |
| F-012 | Permission Preserve | File mode preserved | Create file with mode 0640 | Same mode in other producers |
| F-013 | SUID Strip | SUID/SGID stripped | Create file with mode 4755 | Mode 0755 in other producers (SUID removed) |
| F-014 | Nested Directories | Deep paths work | Create `a/b/c/d/e/file.txt` | Full path synced |
| F-015 | Special Chars | Unicode filenames | Create file with unicode name | Syncs correctly |
| F-016 | Spaces in Names | Whitespace handling | Create `file with spaces.txt` | Syncs correctly |
| F-017 | Symlink Reject | Symlinks not followed | Create symlink to file | Symlink ignored, not synced |
| F-018 | Rapid Rename Cycles | File renamed a->b->c->d->final | Rename file multiple times rapidly | Final name propagated correctly |

### 1.2 Diode Mode

| Test ID | Test Name | Description | Steps | Expected |
| --------- | ----------- | ------------- | ------- | ---------- |
| D-001 | Diode Write | Diode producer can write | Create file in untrusted-vm | Syncs to at least one other producer |
| D-002 | Diode No Receive | Diode doesn't receive | Create file in trusted-vm | NOT synced to untrusted-vm |
| D-003 | Diode Ignore Existing | Existing files not overwritten | File exists in trusted-vm, create same name in diode | Diode file skipped |
| D-004 | Diode Delete Not Propagated | Diode deletions stay local | Delete file in diode share | File remains in other producers |
| D-005 | Diode Rename Not Propagated | Diode renames stay local | Rename file in diode share | Original name stays in other producers |

### 1.3 Scan Tests (Host)

| Test ID | Test Name | Description | Steps | Expected |
| --------- | ----------- | ------------- | ------- | ---------- |
| S-001 | Clean File Pass | Clean files sync | Create normal file | File in other producers |
| S-002 | EICAR Block | Malware blocked | Create EICAR test file | File NOT in other producers |
| S-003 | Quarantine Action | Infected quarantined | EICAR with quarantine config | File in quarantine/, mode 000, root:root |

### 1.4 Scan Tests (VM via vsock)

| Test ID | Test Name | Description | Steps | Expected |
| --------- | ----------- | ------------- | ------- | ---------- |
| V-001 | PING via Proxy | Proxy forwards PING | Guest sends zPING | Receives PONG |
| V-002 | VERSION via Proxy | Proxy forwards VERSION | Guest sends zVERSION | Receives version string |
| V-003 | INSTREAM Clean | Clean file scans | Guest sends file via INSTREAM | Receives OK |
| V-004 | INSTREAM Infected | Infected detected | Guest sends EICAR via INSTREAM | Receives FOUND |
| V-005 | SCAN Blocked | SCAN command rejected | Guest sends zSCAN/path | Receives "Command not allowed" |
| V-006 | SHUTDOWN Blocked | SHUTDOWN rejected | Guest sends zSHUTDOWN | Rejected, clamd still running |
| V-007 | RELOAD Blocked | RELOAD rejected | Guest sends zRELOAD | Rejected |
| V-008 | Case Sensitivity | Wrong case rejected | Guest sends zping | Rejected |
| V-009 | CONTSCAN Blocked | CONTSCAN rejected | Guest sends zCONTSCAN/path | Rejected |
| V-010 | MULTISCAN Blocked | MULTISCAN rejected | Guest sends zMULTISCAN/path | Rejected |
| V-011 | ALLMATCHSCAN Blocked | ALLMATCHSCAN rejected | Guest sends zALLMATCHSCAN/path | Rejected |

### 1.5 vclient On-Write Scan Tests

| Test ID | Test Name | Description | Steps | Expected |
| --------- | ----------- | ------------- | ------- | ---------- |
| VS-001 | Clean File Passes | vclient allows clean files | Write clean file in monitored dir | File accessible after scan |
| VS-002 | Infected File Blocked | vclient blocks malware | Write EICAR in monitored dir | File removed/quarantined by vclient |

### 1.6 Read-Only Export

| Test ID | Test Name | Description | Steps | Expected |
| --------- | ----------- | ------------- | ------- | ---------- |
| R-001 | Consumer Read (Direct) | Consumer reads file from export | Read file created directly in export | File contents match |
| R-002 | Producer Propagation | Consumer sees file via gate | Create file in producer share | File appears in export via gate |
| R-003 | Consumer Write Blocked | Consumer cannot write | Attempt write to export mount | Permission denied |

---

## 2. Performance Tests

### 2.1 Daemon Overhead

| Test ID | Test Name | Description | Metric |
| --------- | ----------- | ------------- | -------- |
| P-001 | 100KB Throughput | 100KB file propagation | MB/s |
| P-002 | 1MB Throughput | 1MB file propagation | MB/s |
| P-003 | 10MB Throughput | 10MB file propagation | MB/s |
| P-004 | 100MB Throughput | 100MB file propagation | MB/s |
| P-005 | 1000MB Throughput | 1GB file propagation | MB/s |

### 2.2 Scan Overhead

| Test ID | Test Name | Description | Metric |
| --------- | ----------- | ------------- | -------- |
| P-006 | Propagation 10MB | End-to-end propagation for 10MB | ms |
| P-007 | Propagation 100MB | End-to-end propagation for 100MB | ms |
| P-008 | Propagation 1GB | End-to-end propagation for 1GB | ms |
| P-009 | Propagation 5GB | End-to-end propagation for 5GB | ms |

### 2.3 vsock Proxy

| Test ID | Test Name | Description | Metric |
| --------- | ----------- | ------------- | -------- |
| P-010 | Proxy Latency | INSTREAM round-trip via vsock | ms |
| P-011 | Proxy Throughput | Large file scan via vsock | MB/s |
| P-012 | Concurrent Scans | 10 parallel INSTREAM requests | total time |

### 2.4 Large File Performance

| Test ID | Test Name | Description | Metric |
| --------- | ----------- | ------------- | -------- |
| P-013 | 2GB Propagation | 2GB file end-to-end | MB/s |
| P-014 | 4GB Propagation | 4GB file end-to-end | MB/s |
| P-015 | 8GB Propagation | 8GB file end-to-end | MB/s |
| P-016 | 10GB Propagation | 10GB file end-to-end | MB/s |
| P-017 | 12GB Propagation | 12GB file end-to-end | MB/s |
| P-018 | 16GB Propagation | 16GB file end-to-end | MB/s |

---

## 3. Security Tests

### 3.1 Overload / DoS

| Test ID | Test Name | Description | Expected |
| --------- | ----------- | ------------- | ---------- |
| SEC-001 | inotify Queue Overflow | Create 100K files rapidly | Daemon recovers via rescan, exponential backoff |
| SEC-002 | Pending Queue Flood | Create 10K+ files within debounce | Oldest files force-processed |
| SEC-003 | Deep Directory Nesting | Create 1000-level deep path | Handles gracefully or rejects |
| SEC-004 | Long Filename | Create file with 255-char filename | Rejected or handled |
| SEC-005 | Many Small Files | 100K tiny files | Daemon stays responsive |
| SEC-006 | vsock Connection Flood | 100 concurrent vsock connections | Semaphore limits, graceful reject |
| SEC-007 | Slow Client | Client sends data very slowly | Timeouts trigger |
| SEC-008 | Partial Request | Client disconnects mid-transfer | Connection cleaned up |

### 3.2 Bypass Attempts

| Test ID | Test Name | Description | Expected |
| --------- | ----------- | ------------- | ---------- |
| SEC-009 | Symlink to File | Create symlink to `/etc/passwd` | Symlink not propagated |
| SEC-010 | Symlink Replace | Replace regular file with symlink | Symlink rejected |
| SEC-011 | Symlink to Device | Create symlink to `/dev/zero` | Symlink not propagated |
| SEC-012 | Symlink Loop | Create `a->b->a` loop | Symlink not propagated |
| SEC-013 | Symlink Dir Escape | Create `dir->/tmp`, write through it | Symlink not followed |
| SEC-014 | Hardlink Attack | Create hardlink to file | Propagated as separate file (different inode) |
| SEC-015 | FIFO Creation | Create named pipe in share | Not propagated (not regular file) |
| SEC-016 | Device File | Create block/char device | Not propagated (not regular file) |
| SEC-017 | Dotdot in Name | Create `foo..bar` | Allowed (not traversal) |
| SEC-018 | Null Byte in Name | Attempt null byte in filename | Filesystem rejects |
| SEC-019 | Race: Create-Delete | Create and immediately delete | Handled gracefully |
| SEC-020 | Ignore Pattern Escape | Create `.crdownload`, rename to `.txt` | Rename triggers scan |

### 3.3 vsock Proxy Security

| Test ID | Test Name | Description | Expected |
| --------- | ----------- | ------------- | ---------- |
| SEC-021 | Command Injection | Send `zPING\0zSCAN\0/` | Only PING processed |
| SEC-022 | Command Overflow | Send 1MB command | Rejected (max 10 bytes) |
| SEC-023 | Oversized Chunk | INSTREAM chunk > 25MB | Rejected |
| SEC-024 | Protocol Violation | Invalid chunk length | Connection terminated |
| SEC-025 | Wrong Delimiter | zPING with \n instead of \0 | Rejected |
| SEC-026 | Lowercase Command | zinstream | Rejected (case sensitive) |
| SEC-027 | Extra Whitespace | `zPING \0` | Rejected |
