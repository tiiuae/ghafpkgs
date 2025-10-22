#!/usr/bin/env bash
# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

# Development setup script for GhafAudioControl
# This script sets up the build environment and generates compile_commands.json
# for better IDE and language server support (clangd, ccls, etc.)

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${PROJECT_ROOT}/build"

echo "🚀 Setting up GhafAudioControl development environment..."

# Create build directory
if [ -d "$BUILD_DIR" ]; then
    echo "🧹 Cleaning existing build directory..."
    rm -rf "$BUILD_DIR"
fi

mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

echo "🔧 Configuring CMake with development options..."

# Configure with development-friendly options
cmake .. \
    -DCMAKE_BUILD_TYPE=Debug \
    -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
    -DCMAKE_CXX_COMPILER_LAUNCHER="" \
    -DCMAKE_VERBOSE_MAKEFILE=ON

echo "📝 Generating compile_commands.json for language servers..."

# Build just enough to generate the compilation database
cmake --build . --target copy-compile-commands || {
    echo "⚠️  Warning: Could not build copy-compile-commands target, trying manual copy..."
    if [ -f "compile_commands.json" ]; then
        cp "compile_commands.json" "$PROJECT_ROOT/"
        echo "✅ Manually copied compile_commands.json to project root"
    else
        echo "❌ No compile_commands.json found. Dependencies may be missing."
        echo "   This is normal if you don't have all system dependencies installed."
    fi
}

echo ""
echo "✅ Development environment setup complete!"
echo ""
echo "📁 Project structure:"
echo "   Source:     $PROJECT_ROOT"
echo "   Build:      $BUILD_DIR" 
echo "   Headers:    lib/include/GhafAudioControl/"
echo "   App Headers: app/include/ghaf-audio-control-app/"
echo ""

if [ -f "$PROJECT_ROOT/compile_commands.json" ]; then
    echo "🎯 Language server configuration:"
    echo "   ✅ compile_commands.json is available in project root"
    echo "   📝 Your IDE/editor should automatically detect it for:"
    echo "      - Code completion"
    echo "      - Error highlighting" 
    echo "      - Go-to-definition"
    echo "      - Refactoring support"
    echo ""
    echo "🔧 Supported language servers: clangd, ccls, cquery"
else
    echo "⚠️  compile_commands.json not generated (missing dependencies)"
    echo "   Language server features may be limited"
fi

echo ""
echo "🛠️  Development commands:"
echo "   Configure: cmake -S . -B build"
echo "   Build:     cmake --build build"
echo "   Clean:     rm -rf build"
echo "   Re-setup:  ./setup-dev.sh"
