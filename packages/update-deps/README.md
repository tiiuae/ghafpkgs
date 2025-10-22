# update-deps

Automatically update dependencies for all packages in the ghafpkgs repository.

## Overview

This tool automatically detects different package types throughout the repository and uses the appropriate package manager to update dependencies:

- **Rust packages**: Uses `cargo update` (and `cargo upgrade` if available)
- **Go packages**: Uses `go get -u` and `go mod tidy`
- **Python packages**: Uses `uv sync --upgrade` for modern pyproject.toml packages
- **Node.js packages**: Uses `npm update` (for future packages)

## Usage

```bash
# Run from the repository root (lock files only - safe)
nix run .#update-deps

# Upgrade source dependencies (potentially breaking)
nix run .#update-deps -- --upgrade

# Show help
nix run .#update-deps -- --help

# Or build and run manually
nix build .#update-deps
./result/bin/update-deps --upgrade
```

### Command Line Options

- **No flags** (default): Updates only lock files (Cargo.lock, go.sum, uv.lock, package-lock.json)
  - ✅ **Safe**: Preserves existing version constraints in source files
  - ✅ **Recommended**: For regular maintenance and security patches within constraints

- **`-u, --upgrade`**: Updates both source dependencies AND lock files
  - ⚠️ **Potentially Breaking**: May introduce breaking changes
  - ⚠️ **Use Carefully**: Review all changes and test thoroughly before committing
  - 🚀 **Powerful**: Upgrades to latest versions respecting semantic versioning

## Features

### Automatic Detection
- Recursively scans the `packages/` directory
- Automatically detects package types by examining files:
  - `Cargo.toml` → Rust package
  - `go.mod` → Go package
  - `pyproject.toml` → Modern Python package
  - `package.json` → Node.js package
  - `requirements.txt` → Legacy Python package

### Flexible and Extensible
- New packages are automatically discovered on next run
- Adding new package types requires only updating the detection logic
- Gracefully handles missing tools (e.g., cargo-upgrade)
- Provides colored output for better visibility

### Safe Operation
- Only updates lock files, doesn't modify source dependencies
- Provides clear output of what's being updated
- Recommends manual review before committing changes

## Supported Package Managers

| Package Type | Lock Files Only (Default) | Source Upgrade (`--upgrade`) | Files Updated |
|-------------|---------------------------|------------------------------|---------------|
| **Rust**    | `cargo update` | `cargo upgrade` + `cargo update` | `Cargo.lock` + `Cargo.toml` |
| **Go**      | `go get -u=patch` + `go mod tidy` | `go get -u` + `go mod tidy` | `go.mod`, `go.sum` |
| **Python**  | `uv sync --upgrade` | `uv add --upgrade <deps>` + sync | `uv.lock` + `pyproject.toml` |
| **Node.js** | `npm update` | `npm upgrade` | `package-lock.json` + `package.json` |

## Output Example

### Normal Mode (Lock Files Only)
```
[update-deps] Starting dependency update process (lock files only)...
[update-deps] Repository root: /path/to/ghafpkgs
[update-deps] Found packages directory: /path/to/ghafpkgs/packages
[update-deps] Checking package: packages/rust/ghaf-mem-manager
[update-deps] Updating Rust dependencies in packages/rust/ghaf-mem-manager
[update-deps] Running cargo update...
    Updating crates.io index
     Locking 3 packages to latest compatible versions
[update-deps] Rust dependencies updated successfully
[update-deps] Dependency update process completed!
[update-deps] Lock files updated - please review changes and test builds before committing.
```

### Upgrade Mode (Source Dependencies)
```
[update-deps] Starting dependency update process in UPGRADE MODE...
[update-deps] ⚠️  WARNING: This will upgrade source dependencies and may introduce breaking changes!
[update-deps] ⚠️  Please review changes carefully and test builds before committing.
[update-deps] Repository root: /path/to/ghafpkgs
[update-deps] Checking package: packages/rust/ghaf-mem-manager
[update-deps] Updating Rust dependencies in packages/rust/ghaf-mem-manager
[update-deps] UPGRADE MODE: Updating source dependencies in Cargo.toml
[update-deps] Running cargo upgrade --workspace...
[update-deps] Running cargo update...
[update-deps] Rust dependencies updated successfully
[update-deps] ⚠️  UPGRADE MODE was used - please review ALL changes and test builds!
```

## Integration

The tool is integrated into the ghafpkgs flake and can be used as part of CI/CD pipelines or regular maintenance workflows.

### From Devshell
```bash
nix develop                    # Enter devshell
update-deps                   # Safe mode (lock files only)
update-deps --upgrade         # Upgrade mode (source deps)
```

### Direct Usage
```bash
nix run .#update-deps                    # Safe mode
nix run .#update-deps -- --upgrade      # Upgrade mode
nix run .#update-deps -- --help         # Show help
```

## Recommended Workflow

1. **Regular Maintenance** (weekly/monthly):
   ```bash
   update-deps                # Safe lock file updates
   nix flake check           # Verify everything still builds
   git commit -am "chore: update dependency lock files"
   ```

2. **Major Upgrades** (quarterly/as needed):
   ```bash
   update-deps --upgrade     # Upgrade source dependencies
   nix flake check          # Test for breaking changes
   # Review and fix any breaking changes
   nix build .#<package>    # Test specific packages
   git add -A && git commit -m "feat: upgrade dependencies to latest versions"
   ```

3. **Before Releases**:
   ```bash
   update-deps --upgrade     # Get latest security patches
   # Thorough testing of all packages
   # Update version numbers if needed
   ```
