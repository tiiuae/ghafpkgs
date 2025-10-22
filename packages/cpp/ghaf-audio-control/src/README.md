<!--
    Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
    SPDX-License-Identifier: CC-BY-SA-4.0
-->

## 📝 Table of Contents

- [About Ghaf Audio Control](#about)
- [Dbus Protocol](#dbus_protocol)

## 🧐 About Ghaf Audio Control <a name = "about"></a>

The service allows to control audio in Ghaf. Uses PulseAudio as a backend.

It has UI in GTK and DBus service.

## 🏁 Audio DBus protocol <a name = "dbus_protocol"></a>

DBus info:
  - Object Path: /org/ghaf/Audio
  - Interface Name: org.ghaf.Audio


### Algorithm

1. The client sends SubscribeToDeviceUpdatedSignal command;
2. The service sends to the client all the devices it has, using the DeviceUpdated signal, -- one signaling from the signal for one device. So, if the service has 5 devices -- the signal will be called 5 times;
3. The service periodically sends to the clients updates about the devices via DeviceUpdated signal;
4. The client can send to the service commands to manipulate devices:
  - SetDeviceMute;
  - SetDeviceVolume;
  - MakeDeviceDefault. 
5. When the client wants to disconnect (taskbar reboot, shutting down or something) from the service, it should send UnsubscribeFromDeviceUpdatedSignal -- to free resources.

### Data types

#### DeviceType <a name = "deviceType"></a>

| Value      | Description |
| :---        |    :----:   |
| 0 | Sink |
| 1 | Source |
| 2 | SinkInput |
| 3 | SourceOutput |
| 4 | Meta |

#### EventType <a name = "eventType"></a>

| Value      | Description |
| :---        |    :----:   |
| 0 | Add |
| 1 | Update |
| 2 | Delete |

### Methods

#### SubscribeToDeviceUpdatedSignal

It's the first method, that a client should send to receive the initial list of the devices.

No parameters.

#### UnsubscribeFromDeviceUpdatedSignal

It's the last methot, that a client should send to terminate the session and free resources on the service.

No parameters.

#### SetDeviceMute

This method allows to mute a device.

Parameters:
| Name | Type   | Limits | Note   |
| :--- | :----: | :----: | :----: |
| deviceId | int | 0...IntMax | It should be existing device id |
| deviceType | int | 0..4 | See [DeviceType](#deviceType) |
| mute | bool | 0..1 | true to mute, false to unmute |

Result: int -- 0 is OK, Error otherwise.

#### SetDeviceVolume


This method allows to set volume for a device.

Parameters:
| Name | Type   | Limits | Note   |
| :--- | :----: | :----: | :----: |
| deviceId | int | 0...IntMax | It should be existing device id |
| deviceType | int | 0..4 | See [DeviceType](#deviceType) |
| volume | int | 0..100 |  |

Result: int -- 0 is OK, Error otherwise.

#### MakeDeviceDefault

This method allows to make a device default

Parameters:
| Name | Type   | Limits | Note   |
| :--- | :----: | :----: | :----: |
| deviceId | int | 0...IntMax | It should be existing device id |
| deviceType | int | 0..4 | See [DeviceType](#deviceType) |

Result: int -- 0 is OK, Error otherwise.

### Signals

#### DeviceUpdated

Parameters:
| Name | Type   | Limits | Note   |
| :--- | :----: | :----: | :----: |
| deviceId | int | 0...IntMax | Id, which a client can use to manipulate the device |
| deviceType | int | 0..4 | See [DeviceType](#deviceType) |
| name | string | 0..100 | Device name |
| volume | int | 0..100 | Device volume |
| isMuted | bool | 0..1 | Is the device muted |
| isDefault | bool | 0..1 | Is the decive default |
| event | int | 0..2 | See [EventType](#eventType) |

### XML representation

```
<node>
        <interface name='org.ghaf.Audio'>
            <!--
                Enum: DeviceType
                Values:
                    - 0: Sink
                    - 1: Source
                    - 2: SinkInput
                    - 3: SourceOutput
                    - 4: Meta

                Enum: EventType
                Values:
                    - 0: Add
                    - 1: Update
                    - 2: Delete
            -->

            <method name='SubscribeToDeviceUpdatedSignal' />
            <method name='UnsubscribeFromDeviceUpdatedSignal' />

            <method name='SetDeviceVolume'>
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum -->
                <arg name='volume' type='i' direction='in' />       <!-- min: 0, max: 100 -->

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <method name='SetDeviceMute'>
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum -->
                <arg name='mute' type='b' direction='in' />

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <method name='MakeDeviceDefault'>
                <arg name='id' type='i' direction='in' />
                <arg name='type' type='i' direction='in' />         <!-- See DeviceType enum. Only a Sink or a Source -->

                <arg name='result' type='i' direction='out' />      <!-- result 0 is OK, Error otherwise -->
            </method>

            <signal name='DeviceUpdated'>
                <arg name='id' type='i' />
                <arg name='type' type='i' />                         <!-- See DeviceType enum -->
                <arg name='name' type='s' />
                <arg name='volume' type='i' />                       <!-- min: 0, max: 100 -->
                <arg name='isMuted' type='b' />
                <arg name='isDefault' type='b' />                    <!-- Makes sense only for a Sink and a Source -->
                <arg name='event' type='i' />                        <!-- See EventType enum -->
            </signal>
        </interface>
    </node>
```
