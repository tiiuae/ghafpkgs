#!/usr/bin/env python
# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
from setuptools import setup, find_packages

setup(
    name="packet-gen",
    version="1.0",
    packages=find_packages(),
    scripts=["mdns-flood.py", "bad-checksum.py"],  # The executable script
)
