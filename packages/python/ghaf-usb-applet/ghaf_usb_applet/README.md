# Ghaf USB Applet

A USB panel applet for that provides system tray integration for USB device management in the Ghaf operating system.

## Overview

The Ghaf USB Applet is a system tray application that enables users to monitor and manage USB devices through an intuitive graphical interface. It provides real-time notifications for USB device connection and disconnection events, allowing users to easily attach or detach devices to virtual machines (VMs).

The applet receives notifications from the vhotplug server running on the host system whenever USB device events occur. It also sends passthrough requests to the host for attaching selected devices to specific VMs, enabling secure and seamless USB device management within the Ghaf multi-VM environment.

## Features

**System Tray Integration:** Displays as an indicator in the system tray using AyatanaAppIndicator3
**USB Device Monitoring:** Real-time monitoring of USB device connection and disconnection events
**Device Management:** Provides options to manage USB devices through a context menu
**VM Selection:** Allows selection of virtual machines for USB device assignment
**Notification System:** Shows desktop notifications for USB device events
**Settings Interface:** Configurable settings through a dedicated settings application

### USB applet

The main system tray applet that provides the core USB monitoring functionality.

**Usage:**

```bash
usb_applet [--loglevel LEVEL] [--port PORT]
```

Options:

--loglevel: Set logging level (default: info)
--port: vHotplug server port (default: 2000)


## Dependencies
Python: >=3.8
PyGObject: >=3.44 (for GTK/GObject bindings)
GTK4: For the graphical interface for settings and user notification
AyatanaAppIndicator3, GTK3: For system tray integration, and applet primary  menu



## License

Apache-2.0
