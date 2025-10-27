# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

# CodeQL Configuration for ghafpkgs

This document describes the CodeQL security analysis configuration for the ghafpkgs project.

## Overview

CodeQL is configured to analyze all programming languages used in this project:

- **C/C++** (55 files) - Manual build with Nix
- **Python** (21 files) - No build required
- **Rust** (9 files) - Manual build with Nix
- **Go** (2 files) - Manual build with Nix
- **GitHub Actions** (4 files) - No build required

## Configuration Files

- `.github/workflows/codeql.yml` - Main CodeQL workflow
- `.github/codeql-config.yml` - CodeQL analysis configuration

## Analysis Scope

### Included Paths
- `packages/cpp/` - C/C++ applications and libraries
- `packages/python/` - Python packages and tools
- `packages/rust/` - Rust applications
- `packages/go/` - Go applications
- `nix/` - Nix configuration and build files

### Excluded Paths
- Documentation files (`*.md`, `*.txt`)
- License files
- Binary assets (images, wallpapers)
- Build artifacts (`target/`, `build/`, `dist/`)
- Generated files (`*.pyc`, `__pycache__/`)

## Build Configuration

### No-Build Mode
All languages use no-build mode for simplicity and compatibility:

```yaml
build-mode: none
```

This approach:
1. Analyzes source code without compilation
2. Avoids build complexity and dependencies
3. Works reliably across all environments
4. Provides comprehensive static analysis

### Analysis Scope
CodeQL analyzes source files directly:
- **C/C++**: Source-level analysis without compilation
- **Rust**: Static analysis of .rs files
- **Go**: Analysis of .go source files
- **Python**: Direct source analysis
- **GitHub Actions**: YAML workflow analysis

## Query Suites

CodeQL runs comprehensive security analysis:

- `security-extended` - Extended security queries
- `security-and-quality` - Security and code quality queries

## Packages Analyzed

### C/C++ Packages
- `ghaf-audio-control` - Audio control application
- `vsockproxy` - VM sockets proxy

### Python Packages
- `ghaf-usb-applet` - USB device management
- `gps-websock` - GPS WebSocket service
- `hotplug` - Device hotplug handling
- `ldap-query` - LDAP query utility
- `vhotplug` - Virtual hotplug handler
- `vinotify` - Notification service

### Rust Packages
- `ghaf-mem-manager` - Memory management service
- `ghaf-nw-packet-forwarder` - Network packet forwarding

### Go Packages
- `swtpm-proxy-shim` - TPM proxy service

## Workflow Triggers

CodeQL analysis runs on:
- **Push** to `main` branch
- **Pull requests** to `main` branch
- **Scheduled** weekly runs (Sundays at 4:18 AM UTC)

## Security Coverage

The configuration focuses on detecting:

### All Languages
- Injection vulnerabilities
- Buffer overflows
- Use-after-free errors
- Cross-site scripting (XSS)
- SQL injection
- Unsafe deserialization

### Language-Specific
- **C/C++**: Memory safety issues, buffer overflows
- **Python**: Import issues, unsafe operations
- **Rust**: Unsafe code patterns
- **Go**: Goroutine safety, input validation

## Maintenance

To update CodeQL configuration:

1. **Add new languages**: Update the matrix in `codeql.yml`
2. **Modify build steps**: Update the manual build section
3. **Change query suites**: Modify `codeql-config.yml`
4. **Update paths**: Adjust `paths` and `paths-ignore` in config

## Troubleshooting

### Language Detection Issues
If CodeQL reports missing languages:
- Ensure files exist in the configured paths
- Check that file extensions are correct
- Verify `.github/workflows/` contains YAML files for Actions analysis

### Analysis Warnings
CodeQL may show warnings about unbuilt code:
- This is normal for no-build mode
- Analysis still covers security vulnerabilities
- Source-level analysis is comprehensive

### False Positives
To suppress false positives:
- Add query filters to `codeql-config.yml`
- Use `@suppress` comments in source code
- Exclude specific file patterns

## Related Documentation

- [CodeQL Documentation](https://docs.github.com/en/code-security/code-scanning)
- [Nix Build System](../README.md)
- [Contributing Guidelines](../CONTRIBUTING.md)
