# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  writeShellApplication,
  lib,
  # Rust tools
  cargo,
  # Go tools
  go,
  # Python tools
  uv,
  python3,
  # Node.js tools (for potential future use)
  nodejs,
  nodePackages,
  # Generic tools
  git,
  findutils,
  gnugrep,
  gnused,
  coreutils,
}:

writeShellApplication {
  name = "update-deps";

  meta = {
    description = "Automatically update dependencies for all packages in the repository";
    license = lib.licenses.asl20;
    platforms = lib.platforms.linux;
    mainProgram = "update-deps";
  };

  # Add all necessary tools for different package managers
  runtimeInputs = [
    # Rust ecosystem
    cargo
    # Go ecosystem
    go
    # Python ecosystem
    uv
    python3
    # Node.js ecosystem (for future packages)
    nodejs
    nodePackages.npm
    # System tools
    git
    findutils
    gnugrep
    gnused
    coreutils
  ];

  text = ''
    set -euo pipefail

    # Parse command line arguments
    UPGRADE_SOURCE=false

    while [[ $# -gt 0 ]]; do
      case $1 in
        -u|--upgrade)
          UPGRADE_SOURCE=true
          shift
          ;;
        -h|--help)
          echo "Usage: update-deps [OPTIONS]"
          echo ""
          echo "Automatically update dependencies for all packages in the repository."
          echo ""
          echo "OPTIONS:"
          echo "  -u, --upgrade    Upgrade source dependencies (not just lock files)"
          echo "  -h, --help       Show this help message"
          echo ""
          echo "Examples:"
          echo "  update-deps                 # Update lock files only (safe)"
          echo "  update-deps --upgrade       # Upgrade source dependencies (potentially breaking)"
          echo ""
          exit 0
          ;;
        *)
          echo "Unknown option: $1"
          echo "Use --help for usage information"
          exit 1
          ;;
      esac
    done

    # Colors for output
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    PURPLE='\033[0;35m'
    NC='\033[0m' # No Color

    # Function to print colored output
    log() {
      local color=$1
      shift
      echo -e "''${color}[update-deps]''${NC} $*"
    }

    # Function to detect package type and return update strategy
    detect_package_type() {
      local dir=$1

      if [[ -f "$dir/Cargo.toml" ]]; then
        echo "rust"
      elif [[ -f "$dir/go.mod" ]]; then
        echo "go"
      elif [[ -f "$dir/pyproject.toml" ]]; then
        echo "python"
      elif [[ -f "$dir/package.json" ]]; then
        echo "nodejs"
      elif [[ -f "$dir/requirements.txt" ]]; then
        echo "python-legacy"
      else
        echo "unknown"
      fi
    }

    # Function to update Rust dependencies
    update_rust_deps() {
      local dir=$1
      log "$BLUE" "Updating Rust dependencies in $dir"

      (
        cd "$dir"
        if [[ -f "Cargo.toml" ]]; then
          if [[ "$UPGRADE_SOURCE" == "true" ]]; then
            log "$PURPLE" "UPGRADE MODE: Updating source dependencies in Cargo.toml"

            if command -v cargo-upgrade >/dev/null 2>&1; then
              log "$YELLOW" "Running cargo upgrade --workspace..."
              cargo upgrade --workspace
            else
              log "$RED" "cargo-upgrade not available, falling back to manual upgrade"
              log "$YELLOW" "Consider installing cargo-edit: cargo install cargo-edit"
              log "$YELLOW" "Manually review Cargo.toml for version updates"
            fi
          fi

          log "$YELLOW" "Running cargo update..."
          cargo update

          log "$GREEN" "Rust dependencies updated successfully"
        else
          log "$RED" "No Cargo.toml found in $dir"
          return 1
        fi
      )
    }

    # Function to update Go dependencies
    update_go_deps() {
      local dir=$1
      log "$BLUE" "Updating Go dependencies in $dir"

      (
        cd "$dir"
        if [[ -f "go.mod" ]]; then
          if [[ "$UPGRADE_SOURCE" == "true" ]]; then
            log "$PURPLE" "UPGRADE MODE: Updating source dependencies in go.mod"
            log "$YELLOW" "Running go get -u (upgrading to latest compatible versions)..."
            go get -u ./...
          else
            log "$YELLOW" "Running go get -u (patch updates only)..."
            go get -u=patch ./...
          fi

          log "$YELLOW" "Running go mod tidy..."
          go mod tidy

          log "$GREEN" "Go dependencies updated successfully"
        else
          log "$RED" "No go.mod found in $dir"
          return 1
        fi
      )
    }

    # Function to update Python dependencies (modern pyproject.toml)
    update_python_deps() {
      local dir=$1
      log "$BLUE" "Updating Python dependencies in $dir"

      (
        cd "$dir"
        if [[ -f "pyproject.toml" ]]; then
          if [[ "$UPGRADE_SOURCE" == "true" ]]; then
            log "$PURPLE" "UPGRADE MODE: Upgrading source dependencies in pyproject.toml"

            # Extract dependency names and upgrade them
            if command -v uv >/dev/null 2>&1; then
              log "$YELLOW" "Using uv to upgrade dependencies..."

              # Try to upgrade dependencies using uv
              if grep -q "dependencies" pyproject.toml; then
                # Get dependency names (basic parsing)
                local deps
                deps=$(grep -A 20 "dependencies" pyproject.toml | grep -E '^\s*"[^"]+' | sed 's/.*"\([^"=<>!]*\).*/\1/' | head -10)

                for dep in $deps; do
                  if [[ -n "$dep" && "$dep" != "[" && "$dep" != "]" ]]; then
                    log "$YELLOW" "Upgrading $dep..."
                    uv add "$dep" --upgrade 2>/dev/null || log "$YELLOW" "Could not upgrade $dep, skipping..."
                  fi
                done
              fi
            else
              log "$YELLOW" "uv not available, manual pyproject.toml review recommended"
            fi
          fi

          log "$YELLOW" "Running uv sync to update lock files..."
          if uv sync --upgrade 2>/dev/null || uv lock --upgrade 2>/dev/null; then
            log "$GREEN" "Python dependencies updated with uv"
          else
            log "$YELLOW" "uv sync/lock failed, trying manual approach..."

            if grep -q "dependencies" pyproject.toml; then
              log "$YELLOW" "Found dependencies in pyproject.toml - manual review recommended"
              if [[ "$UPGRADE_SOURCE" == "true" ]]; then
                log "$YELLOW" "For source upgrades, consider running: uv add --upgrade <package-name>"
              fi
            fi
          fi

          log "$GREEN" "Python dependencies processing completed"
        else
          log "$RED" "No pyproject.toml found in $dir"
          return 1
        fi
      )
    }

    # Function to update Node.js dependencies
    update_nodejs_deps() {
      local dir=$1
      log "$BLUE" "Updating Node.js dependencies in $dir"

      (
        cd "$dir"
        if [[ -f "package.json" ]]; then
          if [[ "$UPGRADE_SOURCE" == "true" ]]; then
            log "$PURPLE" "UPGRADE MODE: Upgrading source dependencies in package.json"
            log "$YELLOW" "Running npm upgrade..."
            npm upgrade
          else
            log "$YELLOW" "Running npm update..."
            npm update
          fi

          log "$GREEN" "Node.js dependencies updated successfully"
        else
          log "$RED" "No package.json found in $dir"
          return 1
        fi
      )
    }

    # Function to handle legacy Python packages
    update_python_legacy() {
      local dir=$1
      log "$BLUE" "Found legacy Python package in $dir"
      log "$YELLOW" "Legacy requirements.txt detected - consider migrating to pyproject.toml"
      log "$YELLOW" "Manual dependency updates may be required"
    }

    # Function to update dependencies for a single package
    update_package_deps() {
      local package_dir=$1
      local package_type

      # Skip if not a directory
      [[ -d "$package_dir" ]] || return 0

      # Skip hidden directories and known non-package directories
      local basename
      basename=$(basename "$package_dir")
      [[ "$basename" =~ ^\..*$ ]] && return 0
      [[ "$basename" == "overlays.nix" ]] && return 0
      [[ "$basename" == "flake-module.nix" ]] && return 0
      [[ "$basename" == "default.nix" ]] && return 0

      log "$BLUE" "Checking package: $package_dir"

      # Detect package type
      package_type=$(detect_package_type "$package_dir")

      case "$package_type" in
        rust)
          update_rust_deps "$package_dir"
          ;;
        go)
          update_go_deps "$package_dir"
          ;;
        python)
          update_python_deps "$package_dir"
          ;;
        nodejs)
          update_nodejs_deps "$package_dir"
          ;;
        python-legacy)
          update_python_legacy "$package_dir"
          ;;
        unknown)
          log "$YELLOW" "Unknown package type in $package_dir, skipping..."
          ;;
      esac
    }

    # Function to recursively find and update packages
    find_and_update_packages() {
      local search_dir=$1
      local max_depth=''${2:-3}

      log "$GREEN" "Searching for packages in $search_dir (max depth: $max_depth)"

      # Find all directories up to max depth that might contain packages
      local dirs
      readarray -t dirs < <(find "$search_dir" -maxdepth "$max_depth" -type d)

      for dir in "''${dirs[@]}"; do
        # Check if this directory contains package files
        local has_package_files=false

        if [[ -f "$dir/Cargo.toml" ]] || \
           [[ -f "$dir/go.mod" ]] || \
           [[ -f "$dir/pyproject.toml" ]] || \
           [[ -f "$dir/package.json" ]] || \
           [[ -f "$dir/requirements.txt" ]]; then
          has_package_files=true
        fi

        if [[ "$has_package_files" == "true" ]]; then
          update_package_deps "$dir"
        fi
      done
    }

    # Main function
    main() {
      if [[ "$UPGRADE_SOURCE" == "true" ]]; then
        log "$PURPLE" "Starting dependency update process in UPGRADE MODE..."
        log "$YELLOW" "⚠️  WARNING: This will upgrade source dependencies and may introduce breaking changes!"
        log "$YELLOW" "⚠️  Please review changes carefully and test builds before committing."
      else
        log "$GREEN" "Starting dependency update process (lock files only)..."
      fi

      # Get the repository root (assumes we're running from within the repo)
      local repo_root
      if git rev-parse --show-toplevel >/dev/null 2>&1; then
        repo_root=$(git rev-parse --show-toplevel)
      else
        repo_root=$(pwd)
        log "$YELLOW" "Not in a git repository, using current directory: $repo_root"
      fi

      log "$GREEN" "Repository root: $repo_root"

      # Look for packages directory
      local packages_dir="$repo_root/packages"
      if [[ -d "$packages_dir" ]]; then
        log "$GREEN" "Found packages directory: $packages_dir"
        find_and_update_packages "$packages_dir" 4
      else
        log "$YELLOW" "No packages directory found, searching entire repository"
        find_and_update_packages "$repo_root" 3
      fi

      log "$GREEN" "Dependency update process completed!"
      if [[ "$UPGRADE_SOURCE" == "true" ]]; then
        log "$YELLOW" "⚠️  UPGRADE MODE was used - please review ALL changes and test builds!"
      else
        log "$YELLOW" "Lock files updated - please review changes and test builds before committing."
        log "$BLUE" "Tip: Use --upgrade flag to also upgrade source dependencies (potentially breaking)"
      fi
    }

    # Run main function
    main "$@"
  '';
}
