# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

from setuptools import setup, find_packages

setup(
    name="vhotplug",
    version="1.0",
    packages=find_packages(),
    entry_points={
        "console_scripts": [
            "vhotplug=vhotplug.vhotplug:main",
        ],
    },
)
