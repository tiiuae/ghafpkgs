# Copyright 2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

# Compiler and flags
CXX := g++
CXXFLAGS := -Wall -Wextra -g
PKG_CONFIG_FLAGS := $(shell pkg-config --cflags --libs glib-2.0 gio-2.0)

# Targets
TARGET := dbus-proxy
SRC := dbus-proxy.cpp
OBJ := $(SRC:.cpp=.o)

# Default target
all: $(TARGET)

# Main build rule
$(TARGET): $(OBJ)
	$(CXX) $(CXXFLAGS) $^ -o $@ $(PKG_CONFIG_FLAGS)

# Compile source files
%.o: %.cpp
	$(CXX) $(CXXFLAGS) -c $< -o $@ $(PKG_CONFIG_FLAGS)

# Clean build artifacts
clean:
	rm -f $(OBJ) $(TARGET)

# Rebuild everything
rebuild: clean all

.PHONY: all clean rebuild
