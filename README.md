<!--
    Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
    SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Ghaf Packages

This repository contains packages used in the [Ghaf framework](https://ghaf.dev) - a hardened virtualization platform for edge computing.

## 🚀 Quick Start

```bash
# Clone the repository
git clone https://github.com/tiiuae/ghafpkgs.git
cd ghafpkgs

# Enter development environment
nix develop

# Build a package
nix build .#ghaf-audio-control

# Update all package dependencies
update-deps

# Update with source upgrades (potentially breaking)
update-deps --upgrade
```

## 📦 Package Categories

### 🎨 Art & Themes (`packages/art/`)
Visual assets and themes for Ghaf systems.

- **`ghaf-artwork`** - Ghaf branding and artwork assets
- **`ghaf-theme`** - GTK4 theme for Ghaf desktop environment
- **`ghaf-wallpapers`** - Default wallpapers collection

### 🐍 Python (`packages/python/`)
Python applications and utilities, all modernized with `pyproject.toml` + `uv`.

- **`ghaf-usb-applet`** - USB panel applet for COSMIC (GTK4) with system tray integration
- **`gps-websock`** - GPS endpoint exposed over WebSocket for real-time location data
- **`hotplug`** - QEMU hotplug helper for PCI and USB devices
- **`ldap-query`** - LDAP/Active Directory query tool with GSSAPI auth
- **`vhotplug`** - Virtio hotplug management (external dependency)
- **`vinotify`** - VM file system notification service using inotify

### 🦀 Rust (`packages/rust/`)
High-performance system utilities written in Rust.

- **`ghaf-mem-manager`** - Memory management utilities
- **`ghaf-nw-packet-forwarder`** - Network packet forwarding service

### 🔵 Go (`packages/go/`)
Go-based system services and utilities.

- **`swtmp-proxy-shim`** - Software TPM proxy shim

### ⚡ C++ (`packages/cpp/`)
C++ applications with desktop integration.

- **`ghaf-audio-control`** - Audio control application with Qt6 GUI

### 🛠️ Development Tools (`packages/update-deps/`)
Repository maintenance and development utilities.

- **`update-deps`** - Automatic dependency updater for all package types

## 🔧 Development

### Development Environment

```bash
# Enter development shell with all tools
nix develop

# Available tools in devshell:
# - update-deps (dependency management)
# - reuse (license compliance)
# - cargo (Rust development)
# - go (Go development)
# - nix-fast-build (efficient Nix builds)
# - All package-specific build tools
```

### Building Packages

```bash
# Build specific packages
nix build .#ghaf-audio-control
nix build .#ghaf-usb-applet
nix build .#gps-websock
nix build .#hotplug
nix build .#ghaf-mem-manager

# Build all packages
nix flake check

# Fast parallel builds
nix-fast-build
```

### Dependency Management

The repository includes an automated dependency updater that supports all package types:

```bash
# Safe updates (lock files only)
update-deps

# Full upgrades (potentially breaking)
update-deps --upgrade

# Show help
update-deps --help
```

**Supported Package Managers:**
- **Rust**: `cargo update` / `cargo upgrade`
- **Go**: `go get -u=patch` / `go get -u`
- **Python**: `uv sync` / `uv add --upgrade`
- **Node.js**: `npm update` / `npm upgrade`

### Package Development

#### Adding New Packages

1. **Choose appropriate category** (`art/`, `python/`, `rust/`, `go/`, `cpp/`)
2. **Create package directory** with `default.nix` or `package.nix`
3. **Add to category's `default.nix`** export list
4. **Use modern packaging standards**:
   - Python: `pyproject.toml` with `uv` and `hatchling`
   - Rust: `Cargo.toml` with workspace support
   - Go: `go.mod` with proper module structure

#### Python Package Standards

All Python packages use modern tooling:

```toml
# pyproject.toml example
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "package-name"
version = "1.0.0"
dependencies = ["dep1>=1.0", "dep2>=2.0"]

[project.scripts]
command-name = "module.main:main"
```

#### Package.nix Structure

```nix
# For Python packages
{
  buildPythonApplication,
  hatchling,
  uv,
  # dependencies...
}:
buildPythonApplication {
  pname = "package-name";
  version = "1.0.0";

  build-system = [ hatchling uv ];
  propagatedBuildInputs = [ /* runtime deps */ ];

  src = ./package-source;
  pyproject = true;
  doCheck = false;

  meta = {
    description = "Package description";
    license = lib.licenses.asl20;
    platforms = platforms.linux;
  };
}
```

## 🏗️ Architecture

### Repository Structure

```
ghafpkgs/
├── packages/
│   ├── art/           # Visual assets and themes
│   ├── cpp/           # C++ applications
│   ├── go/            # Go services
│   ├── python/        # Python applications
│   ├── rust/          # Rust utilities
│   ├── update-deps/   # Development tools
│   └── flake-module.nix
├── nix/
│   └── devshell.nix   # Development environment
├── flake.nix          # Main flake configuration
└── README.md          # This file
```

### Build System

- **Nix Flakes** for reproducible builds and dependency management
- **Category-based organization** with dedicated `default.nix` in each category
- **Modern package managers**: uv (Python), cargo (Rust), go modules (Go)
- **Automated dependency management** with `update-deps` tool

### Integration Points

Packages are designed to integrate with:
- **Ghaf Framework** - Main virtualization platform
- **NixOS configurations** - System-level integration
- **Development workflows** - CI/CD and testing
- **Security frameworks** - Hardened virtualization context

## 🔄 Maintenance

### Regular Maintenance

```bash
# Weekly dependency updates (safe)
update-deps
nix flake check
git commit -am "chore: update dependency lock files"

# License compliance check
reuse lint

# Code formatting
nix fmt
```

### Major Updates

```bash
# Quarterly dependency upgrades (potentially breaking)
update-deps --upgrade
nix flake check
# Review and fix any breaking changes
git commit -am "feat: upgrade dependencies to latest versions"
```

### Release Process

1. **Update dependencies**: `update-deps --upgrade`
2. **Run full tests**: `nix flake check`
3. **Update documentation** if needed
4. **Tag release**: Follow semantic versioning
5. **Update Ghaf framework** integration

## 🤝 Contributing

### Development Workflow

1. **Fork the repository**
2. **Create feature branch**: `git checkout -b feature/new-package`
3. **Enter dev environment**: `nix develop`
4. **Make changes** following the patterns in existing packages
5. **Test thoroughly**: `nix build .#your-package`
6. **Update dependencies**: `update-deps`
7. **Run checks**: `nix flake check && reuse lint`
8. **Submit pull request**

### Code Standards

- **License compliance**: All files must have SPDX headers (`reuse lint`)
- **Modern packaging**: Use latest standards (pyproject.toml, Cargo.toml, go.mod)
- **Documentation**: Include README.md for complex packages
- **Testing**: Ensure packages build successfully with `nix flake check`

## 📄 License

Licensed under **Apache-2.0**. See [LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt) for details.

This project follows the [REUSE specification](https://reuse.software/) for license compliance.

## 🔗 Related Projects

- **[Ghaf Framework](https://github.com/tiiuae/ghaf)** - Main virtualization platform
- **[Ghaf Documentation](https://ghaf.dev)** - Project documentation and guides
- **[TII Open Source](https://github.com/tiiuae)** - Technology Innovation Institute projects

## 📞 Support

- **Documentation**: [ghaf.dev](https://ghaf.dev)
- **Issues**: [GitHub Issues](https://github.com/tiiuae/ghafpkgs/issues)
- **Community**: [Ghaf Community](https://github.com/tiiuae/ghaf/discussions)

---

**Ghaf Packages** - Hardened virtualization platform components
🛡️ Security-focused • 🚀 Edge-optimized • 🔧 Developer-friendly
