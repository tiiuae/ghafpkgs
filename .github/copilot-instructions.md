<!--
SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
SPDX-License-Identifier: Apache-2.0
-->

# Ghafpkgs Development Instructions

**Ghafpkgs** is a collection of packages for the Ghaf Framework - a hardened virtualization platform for edge computing. This is a **multi-language repository** containing packages in Nix, Python, Rust, Go, C++, and Shell.

**CRITICAL: Always reference these instructions first and fallback to search or bash commands only when you encounter unexpected information that does not match the info here.**

## Serena Code Analysis (MCP Server)

**IMPORTANT: At the start of every session, check if Serena is available by running `serena-get_current_config` or `serena-check_onboarding_performed`. If Serena is enabled, use it for ALL code analysis and investigation tasks.**

### When to Use Serena

Use Serena's semantic analysis tools for:
- **Understanding package structure**: Navigate Nix package definitions, analyze dependencies
- **Multi-language code investigation**: Analyze Python modules, Rust crates, Go packages, C++ classes
- **Finding symbols across languages**: Locate functions, classes, methods in any supported language
- **Tracking package relationships**: Find where packages are referenced, imported, or used
- **Build system analysis**: Understand Nix expressions, flake structure, and overlay patterns

### Standard Serena Workflow for Ghafpkgs

1. **Check session state**: `serena-check_onboarding_performed` to verify Serena is ready
2. **List available memories**: `serena-list_memories` to see existing project knowledge
   - Look for: `project-overview`, `package-details`, `development-guide`, `architecture-and-patterns`, `ghaf-integration`
3. **Read relevant memories**: `serena-read_memory` for context on specific areas
4. **Find symbols**: Use `serena-find_symbol` to locate functions, classes, or package definitions
5. **Analyze references**: Use `serena-find_referencing_symbols` to see usage across codebase
6. **Get file overview**: Use `serena-get_symbols_overview` to understand module structure
7. **Search patterns**: Use `serena-search_for_pattern` for flexible code searches
8. **Create memories**: Use `serena-write_memory` to document findings for future sessions

### Language-Specific Serena Usage

**Nix Packages**:
```bash
# Find a package definition
serena-find_symbol --name_path_pattern "ghaf-audio-control" --relative_path "packages/"

# Get overview of package category
serena-get_symbols_overview --relative_path "packages/python/default.nix"

# Search for buildPythonApplication patterns
serena-search_for_pattern --substring_pattern "buildPythonApplication" --restrict_search_to_code_files true
```

**Python Packages**:
```bash
# Find Python class definitions
serena-find_symbol --name_path_pattern "USBApplet" --relative_path "packages/python/"

# Get module overview
serena-get_symbols_overview --relative_path "packages/python/ghaf-usb-applet/ghaf_usb_applet/src/ghaf_usb_applet/applet.py"

# Find all references to a class
serena-find_referencing_symbols --name_path "USBApplet" --relative_path "packages/python/ghaf-usb-applet/ghaf_usb_applet/src/ghaf_usb_applet/applet.py"
```

**Rust Packages**:
```bash
# Find Rust modules
serena-find_symbol --name_path_pattern "forward_impl" --relative_path "packages/rust/ghaf-nw-packet-forwarder/"

# Analyze Rust structure
serena-get_symbols_overview --relative_path "packages/rust/ghaf-nw-packet-forwarder/src/main.rs" --depth 1

# Search for specific patterns
serena-search_for_pattern --substring_pattern "async fn" --relative_path "packages/rust/"
```

**C++ Packages**:
```bash
# Find C++ classes
serena-find_symbol --name_path_pattern "AudioControl" --relative_path "packages/cpp/ghaf-audio-control/"

# Get class overview with methods
serena-get_symbols_overview --relative_path "packages/cpp/ghaf-audio-control/src/lib/include/GhafAudioControl/AudioControl.hpp" --depth 1

# Find Qt connections
serena-search_for_pattern --substring_pattern "connect\\(" --relative_path "packages/cpp/"
```

**Go Packages**:
```bash
# Find Go functions
serena-find_symbol --name_path_pattern "main" --relative_path "packages/go/swtpm-proxy-shim/"

# Get package overview
serena-get_symbols_overview --relative_path "packages/go/swtpm-proxy-shim/server.go"
```

**Note**: If Serena is not available in the session, fall back to standard grep/view/glob tools for code analysis.

## Context7 Documentation Lookup (MCP Server)

**IMPORTANT: Use Context7 for up-to-date documentation on technologies used in this multi-language project.**

### When to Use Context7

Use Context7 for:
- **Nix/NixOS**: Package building, derivations, overlays, flake structure
- **Python**: pyproject.toml, hatchling, uv (dependency management)
- **Rust**: Cargo, Crane (Nix Rust builder), common Rust libraries
- **Go**: Go modules, buildGoModule in Nix
- **C++**: Qt6, CMake, Meson build systems
- **Framework integration**: Understanding how packages integrate with Ghaf

### Standard Context7 Workflow

1. **Resolve library ID**: Use `context7-resolve-library-id` to find the correct library
2. **Get documentation**: Use `context7-get-library-docs` with the resolved library ID
   - Use `mode='code'` (default) for API references, build patterns, code examples
   - Use `mode='info'` for conceptual guides, tutorials, architecture
3. **Iterate with pagination**: If context is insufficient, use `page=2`, `page=3`, etc.
4. **Focus with topics**: Use the `topic` parameter to narrow documentation scope

### Example Context7 Commands

**Nix/Nixpkgs**:
```bash
# Resolve Nixpkgs library
context7-resolve-library-id --libraryName "nixpkgs"

# Get buildPythonApplication documentation
context7-get-library-docs --context7CompatibleLibraryID "/NixOS/nixpkgs" --mode "code" --topic "buildPythonApplication"

# Get Crane documentation for Rust
context7-get-library-docs --context7CompatibleLibraryID "/ipetkov/crane" --mode "code" --topic "buildPackage"

# Get buildGoModule documentation
context7-get-library-docs --context7CompatibleLibraryID "/NixOS/nixpkgs" --mode "code" --topic "buildGoModule"
```

**Python Ecosystem**:
```bash
# Get hatchling documentation
context7-get-library-docs --context7CompatibleLibraryID "/pypa/hatch" --mode "code" --topic "build-system"

# Get uv documentation
context7-get-library-docs --context7CompatibleLibraryID "/astral-sh/uv" --mode "code" --topic "dependency"

# Get GTK4 Python bindings
context7-get-library-docs --context7CompatibleLibraryID "/GNOME/pygobject" --mode "code" --topic "gtk4"
```

**Rust Ecosystem**:
```bash
# Get Rust language docs
context7-get-library-docs --context7CompatibleLibraryID "/rust-lang/rust" --mode "code" --topic "async"

# Get tokio documentation
context7-get-library-docs --context7CompatibleLibraryID "/tokio-rs/tokio" --mode "code" --topic "runtime"
```

**C++ Ecosystem**:
```bash
# Get Qt6 documentation
context7-get-library-docs --context7CompatibleLibraryID "/qt/qt" --mode "code" --topic "widgets"

# Get CMake documentation
context7-get-library-docs --context7CompatibleLibraryID "/Kitware/cmake" --mode "code" --topic "find_package"
```

**Note**: Always resolve library IDs first unless you know the exact Context7-compatible ID format.

## Project Overview

This repository contains packages for the Ghaf Framework, organized by language:

- **`packages/art/`** - Visual assets and themes (Nix)
- **`packages/python/`** - Python applications with pyproject.toml + uv + hatchling
- **`packages/rust/`** - Rust utilities built with Crane
- **`packages/go/`** - Go services with standard Go modules
- **`packages/cpp/`** - C++ applications with Qt6, CMake, Meson, or Make
- **`packages/update-deps/`** - Dependency management tool (Nix + Shell)
- **`nix/`** - Development environment and tooling
- **`flake.nix`** - Main flake configuration

### Key Characteristics

- **Multi-language**: Nix, Python, Rust, Go, C++, Shell
- **Modern tooling**: uv (Python), Crane (Rust), Go modules, Qt6 (C++)
- **Flake-based**: Reproducible builds with Nix flakes
- **Category organization**: Each language has its own directory with `default.nix`
- **Dual exports**: Both perSystem packages and overlay for integration
- **Ghaf integration**: Packages consumed by the main Ghaf framework repository

## Prerequisites and Setup

### Initial Setup
- Install Nix package manager: `curl -L https://nixos.org/nix/install | sh`
- Enable flakes: `echo "experimental-features = nix-command flakes" >> ~/.config/nix/nix.conf`
- For cross-compilation (aarch64): Set up remote builder or use QEMU emulation

### Development Environment
Enter the development shell to access all tools:
```bash
nix develop
```

This provides:
- **Nix tools**: nix-eval-jobs, nix-fast-build, nix-output-monitor, nix-tree
- **Build tools**: gcc, cmake-language-server, pkg-config, systemd
- **Language tools**: clippy, cargo-edit (Rust), reuse (license compliance)
- **Repository tools**: update-deps, treefmt
- **All packages**: Available for integration testing

## Code Quality Standards

### **ALWAYS Strip Trailing Whitespace**
- **Automatically remove trailing whitespace** from any files you create or modify
- **Use sed command**: `sed -i 's/[[:space:]]*$//' filename` to clean files
- **Project-wide consistency**: Maintain clean, professional code formatting standards

### **File Formatting Requirements**
- **All commits must be properly formatted** using treefmt before making a PR
- **Run formatting**: `nix fmt` or `nix fmt -- --fail-on-change`
- **License headers**: Always add SPDX headers to new files (Apache-2.0 for code)
- **REUSE compliance**: `reuse lint` must pass

### **License Header Format**
```
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
```
(Adjust comment syntax for language: `//` for C++/Rust/Go, `#` for Python/Nix/Shell)

## Development Workflow

### Build Commands

**General**:
- **Development shell**: `nix develop` - Enters environment with all tools
- **Build specific package**: `nix build .#<package-name>` - e.g., `nix build .#ghaf-audio-control`
- **Build all packages**: `nix flake check` - Validates all packages
- **Fast parallel builds**: `nix-fast-build` - Efficient multi-package building
- **Format code**: `nix fmt` - Formats all files using treefmt
- **Check formatting**: `nix fmt -- --fail-on-change` - Verifies formatting

**Available Packages** (see `nix flake show` for full list):
- Art: `ghaf-artwork`, `ghaf-theme`, `ghaf-wallpapers`
- Python: `ghaf-usb-applet`, `gps-websock`, `ldap-query`, `vinotify`
- Rust: `ghaf-kill-switch-app`, `ghaf-mem-manager`, `ghaf-nw-packet-forwarder`
- Go: `swtpm-proxy-shim`
- C++: `dbus-proxy`, `ghaf-audio-control`, `vsockproxy`
- Tools: `update-deps`

### Language-Specific Workflows

#### Python Packages

**Directory structure**: `packages/python/<name>/<name>/`

**Development**:
```bash
cd packages/python/my-package/my-package
uv sync                      # Install dependencies
uv add some-dependency       # Add new dependency
.venv/bin/python -m my_package.main  # Run locally
```

**Build with Nix**:
```bash
nix build .#my-package       # Build package
nix develop .#my-package     # Enter build environment
```

**Testing changes**:
```bash
# After modifying source
nix build .#my-package --rebuild
# Test the built binary
./result/bin/my-command
```

#### Rust Packages

**Directory structure**: `packages/rust/<name>/`

**Development**:
```bash
cd packages/rust/my-package
cargo build                  # Build with Cargo
cargo test                   # Run tests
cargo clippy                 # Linting
cargo update                 # Update dependencies
```

**Build with Nix**:
```bash
nix build .#my-package       # Build with Crane
```

**Update Cargo.lock**:
```bash
cd packages/rust/my-package
cargo update                 # Update within constraints
cargo upgrade                # Upgrade to latest (requires cargo-edit)
```

#### Go Packages

**Directory structure**: `packages/go/<name>/`

**Development**:
```bash
cd packages/go/my-package
go build ./cmd/...           # Build
go test ./...                # Test
go mod tidy                  # Clean dependencies
```

**Build with Nix**:
```bash
nix build .#my-package       # Build with buildGoModule
```

**Update dependencies**:
```bash
cd packages/go/my-package
go get -u=patch              # Patch updates
go get -u                    # All updates
go mod tidy
```

**Note**: After updating go.mod/go.sum, you may need to update vendorHash in default.nix

#### C++ Packages

**Directory structure**: `packages/cpp/<name>/`

**Build systems vary**:
- **CMake**: ghaf-audio-control
- **Meson**: vsockproxy
- **Make**: dbus-proxy

**Development** (example with CMake):
```bash
cd packages/cpp/ghaf-audio-control
mkdir build && cd build
cmake ..
make
./ghaf-audio-control
```

**Build with Nix**:
```bash
nix build .#ghaf-audio-control
```

### Dependency Management

**Update all dependencies** (safe, respects version constraints):
```bash
update-deps
```

**Upgrade all dependencies** (potentially breaking):
```bash
update-deps --upgrade
```

**Per-language updates** (manual):
```bash
# Python
cd packages/python/<name>/<name>
uv sync && uv add --upgrade <package>

# Rust
cd packages/rust/<name>
cargo update && cargo upgrade

# Go
cd packages/go/<name>
go get -u && go mod tidy
```

**Update flake inputs**:
```bash
nix flake update              # Update all inputs
nix flake lock --update-input nixpkgs  # Update specific input
```

### Code Quality Checks

**CRITICAL: All commits must pass these checks before making a PR**

```bash
# Format all code
nix fmt

# Check formatting without changes
nix fmt -- --fail-on-change

# Check license compliance
reuse lint

# Build all packages
nix flake check

# Fast parallel builds
nix-fast-build
```

**Pre-commit hooks** are available but not mandatory in devshell. CI will enforce:
- treefmt formatting
- REUSE license compliance
- Package builds

## Testing

### Build Testing

**Single package**:
```bash
nix build .#<package-name> --print-build-logs
```

**All packages**:
```bash
nix flake check              # All packages + checks
nix-fast-build              # Parallel builds
```

**Specific category**:
```bash
# Build all Python packages
nix build .#ghaf-usb-applet .#gps-websock .#ldap-query .#vinotify
```

### Integration Testing with Ghaf

**Test locally** before submitting to ghafpkgs:
```bash
# In a separate Ghaf repository clone
cd ../ghaf
# Edit flake.nix to point to local ghafpkgs
# ghafpkgs.url = "path:/path/to/ghafpkgs";
nix build .#packages.x86_64-linux.lenovo-x1-carbon-gen11-debug
```

**Test with PR branch**:
```nix
# In ghaf/flake.nix
ghafpkgs.url = "github:tiiuae/ghafpkgs?ref=pull/142/head";
```

### Language-Specific Testing

**Python**:
```bash
cd packages/python/<name>/<name>
uv run pytest                # If tests are set up
.venv/bin/python -m module.main --help  # Manual testing
```

**Rust**:
```bash
cd packages/rust/<name>
cargo test                   # Unit tests
cargo test --test integration  # Integration tests
```

**Go**:
```bash
cd packages/go/<name>
go test ./...                # All tests
go test -v ./cmd/...         # Verbose output
```

**C++**:
```bash
cd packages/cpp/<name>
# Tests depend on build system
# CMake: ctest
# Custom: check package-specific README
```

## Adding New Packages

### General Process

1. **Choose category**: art, python, rust, go, cpp, or create new
2. **Create package directory**: `packages/<category>/<package-name>/`
3. **Add package definition**: `default.nix` or `package.nix`
4. **Update category exports**: Modify `packages/<category>/default.nix`
5. **Add to overlay** (if needed): Update `packages/flake-module.nix`
6. **Test build**: `nix build .#<package-name>`
7. **Run checks**: `nix flake check && reuse lint`
8. **Document**: Add to README if significant

### Python Package Template

**Structure**:
```
packages/python/my-package/
├── package.nix          # Nix package definition
└── my-package/          # Python package root
    ├── pyproject.toml   # PEP 621 metadata
    ├── uv.lock         # Locked dependencies
    └── src/
        └── my_package/
            ├── __init__.py
            └── main.py
```

**pyproject.toml**:
```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "my-package"
version = "1.0.0"
dependencies = [
    "dep1>=1.0",
]

[project.scripts]
my-command = "my_package.main:main"
```

**package.nix**:
```nix
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildPythonApplication,
  hatchling,
  uv,
  dep1,
}:
buildPythonApplication {
  pname = "my-package";
  version = "1.0.0";

  build-system = [ hatchling uv ];
  propagatedBuildInputs = [ dep1 ];

  src = ./my-package;
  pyproject = true;
  doCheck = false;

  meta = {
    description = "Package description";
    license = lib.licenses.asl20;
    platforms = platforms.linux;
  };
}
```

**Add to category**:
```nix
# packages/python/default.nix
{
  my-package = python3Packages.callPackage ./my-package/package.nix { };
  # ... other packages
}
```

### Rust Package Template

**Structure**:
```
packages/rust/my-package/
├── default.nix
├── Cargo.toml
├── Cargo.lock
└── src/
    └── main.rs
```

**default.nix**:
```nix
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  crane,
  lib,
  stdenv,
}:
let
  craneLib = crane.mkLib stdenv.hostPlatform;
  src = craneLib.cleanCargoSource ./.;
in
craneLib.buildPackage {
  inherit src;
  pname = "my-package";
  version = "1.0.0";

  strictDeps = true;

  meta = {
    description = "Package description";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
  };
}
```

**Add to category**:
```nix
# packages/rust/default.nix
{
  my-package = callPackage ./my-package { inherit crane; };
  # ... other packages
}
```

### Go Package Template

**Structure**:
```
packages/go/my-package/
├── default.nix
├── go.mod
├── go.sum
└── cmd/
    └── my-package/
        └── main.go
```

**default.nix**:
```nix
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  buildGoModule,
  lib,
}:
buildGoModule {
  pname = "my-package";
  version = "1.0.0";

  src = ./.;

  vendorHash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  # Run build once to get correct hash from error message

  meta = {
    description = "Package description";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
  };
}
```

**Add to category**:
```nix
# packages/go/default.nix
{
  my-package = callPackage ./my-package { };
  # ... other packages
}
```

## Ghaf Framework Integration

This repository is consumed by the main Ghaf framework as a flake input. Understanding this relationship is crucial for development.

### How Ghafpkgs is Used

**In Ghaf's flake.nix**:
```nix
inputs.ghafpkgs = {
  url = "github:tiiuae/ghafpkgs";
  inputs.nixpkgs.follows = "nixpkgs";
  # ... other follows
};
```

**Overlay composition**:
```nix
# Ghaf includes ghafpkgs overlay
nixpkgs.overlays = [
  inputs.ghafpkgs.overlays.default
  # ... other overlays
];
```

**Package usage in Ghaf VMs**:
- **gui-vm**: Uses ghaf-theme, ghaf-artwork, ghaf-wallpapers
- **audio-vm**: Uses ghaf-audio-control
- **net-vm**: Uses ghaf-nw-packet-forwarder, gps-websock
- **Various VMs**: Use dbus-proxy, vsockproxy for communication
- **System services**: vinotify, ghaf-mem-manager, swtpm-proxy-shim

### Testing Integration

**Before submitting PR**:
1. Build package in ghafpkgs: `nix build .#<package>`
2. Test in Ghaf context (if possible)
3. Document breaking changes
4. Notify Ghaf maintainers of significant changes

**For Ghaf maintainers testing ghafpkgs PR**:
```nix
# In ghaf/flake.nix
ghafpkgs.url = "github:tiiuae/ghafpkgs?ref=pull/<number>/head";
```

## Common Tasks

### Updating Package Version

1. Update version in source (pyproject.toml, Cargo.toml, etc.)
2. Update version in package.nix/default.nix
3. Update dependencies: `update-deps` or language-specific command
4. Test build: `nix build .#<package>`
5. Commit with semantic versioning message

### Fixing Build Issues

**Python**:
- Check pyproject.toml dependencies match package.nix
- Verify Nixpkgs has required Python packages
- Ensure native dependencies (systemd, pkg-config) are available
- Update uv.lock: `uv sync`

**Rust**:
- Update Cargo.lock: `cargo update`
- Check if Crane is properly passed to derivation
- Verify all Rust dependencies are available

**Go**:
- Update go.sum: `go mod tidy`
- Recalculate vendorHash (set empty, build, use error hash)
- Check Go version compatibility

**C++**:
- Verify build system (CMake, Meson, Make) is correct
- Check Qt6 modules are included
- Ensure pkg-config can find dependencies

### Debugging with Nix

**Enter build environment**:
```bash
nix develop .#<package>
# Or with more control:
nix develop --command bash
```

**Show build logs**:
```bash
nix build .#<package> --print-build-logs
```

**Show trace on errors**:
```bash
nix build .#<package> --show-trace
```

**Check package evaluation**:
```bash
nix eval .#packages.x86_64-linux.<package>.name
```

## Troubleshooting

### Common Issues

**"Package not found in overlay"**:
- Check package is exported in `packages/<category>/default.nix`
- Verify package is listed in `packages/flake-module.nix` overlay
- Run `nix flake show` to see available packages

**"Builder failed with exit code 1"**:
- Check logs: `nix build .#<package> --print-build-logs`
- Enter build env: `nix develop .#<package>`
- Verify dependencies are correctly specified

**"Hash mismatch" (Python/Go)**:
- For Python: Update uv.lock with `uv sync`
- For Go: Update go.sum with `go mod tidy`, recalculate vendorHash
- For Rust: Update Cargo.lock with `cargo update`

**"License check failed"**:
- Add SPDX headers to all new files
- Run `reuse lint` to identify missing headers
- Use correct license: Apache-2.0 for code, CC-BY-SA-4.0 for docs

**"Format check failed"**:
- Run `nix fmt` to format all files
- Use `nix fmt -- --fail-on-change` to verify
- Check for trailing whitespace: `sed -i 's/[[:space:]]*$//' file`

**"Cross-compilation failed"**:
- Ensure aarch64-linux is in supported systems
- May need remote builder or QEMU binfmt
- Check if package supports cross-compilation

## Project Standards

### Commit Messages
Use conventional commits:
- `feat:` - New feature or package
- `fix:` - Bug fix
- `chore:` - Maintenance (dependencies, formatting)
- `docs:` - Documentation changes
- `refactor:` - Code refactoring
- `test:` - Test additions/changes

Examples:
- `feat(python): add new ghaf-monitor package`
- `fix(rust): resolve ghaf-mem-manager build error`
- `chore: update dependencies with update-deps`

### Code Style

**Nix**:
- Follow Nixpkgs conventions
- Use `lib.mkOption`, `lib.mkEnableOption` for options
- Prefer `buildPythonApplication` over `buildPythonPackage` for executables

**Python**:
- PEP 8 style (enforced by ruff via treefmt)
- Type hints encouraged
- Use pyproject.toml (PEP 621)

**Rust**:
- Rustfmt (enforced by treefmt)
- Clippy lints enabled
- Follow Rust API guidelines

**Go**:
- gofmt (enforced by treefmt)
- Follow Go style guide
- Use conventional project layout

**C++**:
- clang-format (if configured)
- Qt conventions for Qt projects
- Modern C++ (C++17 or later)

### Documentation

**Package README**:
- Required for complex packages
- Include: purpose, dependencies, usage, configuration
- Example: `packages/cpp/ghaf-audio-control/README.md`

**Code comments**:
- Use SPDX headers on all files
- Comment complex logic, not obvious code
- Document public APIs and interfaces

## Resources

- **Ghaf Framework**: https://github.com/tiiuae/ghaf
- **Ghaf Documentation**: https://ghaf.tii.ae
- **Nixpkgs Manual**: https://nixos.org/manual/nixpkgs/
- **Nix Flakes**: https://nixos.wiki/wiki/Flakes
- **REUSE Specification**: https://reuse.software/
