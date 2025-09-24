# Copyright 2022-2025 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0

from dataclasses import dataclass
from enum import Enum
from typing import Any, Dict, List, Optional
from abc import ABC, abstractmethod

from vsock_bridge.logger import logger


def _str_ok(v: Any, name: str) -> str:
    if not isinstance(v, str) or not v:
        raise ValueError(f"'{name}' must be non-empty str")
    return v


def _is_list_of_str(v: Any) -> bool:
    return isinstance(v, list) and all(isinstance(x, str) for x in v)


class PassthroughStatus(str, Enum):
    OK = "ok"
    DENIED = "denied"
    ERROR = "error"


class Message(ABC):
    @abstractmethod
    def to_json(self) -> dict:
        """Convert to dict representation."""

    @staticmethod
    @abstractmethod
    def from_message(pl: dict):
        """Build instance from payload dict."""

    def print(self):
        logger.info(self.to_json())


@dataclass
class DeviceConnected(Message):
    MSG_TYPE = "device_connected"
    _data: Dict[str, Any]

    def __init__(
        self,
        device_id: str,
        vendor: str,
        product: str,
        permitted_vms: List[str],
        current_vm: Optional[str],
    ) -> None:
        if not _is_list_of_str(permitted_vms):
            raise ValueError("device.permitted_vms must be list[str]")
        if current_vm is not None and not isinstance(current_vm, str):
            raise ValueError("device.current_vm must be str | None")
        self._data = {
            "device": {
                "device_id": _str_ok(device_id, "device_id"),
                "vendor": _str_ok(vendor, "vendor"),
                "product": _str_ok(product, "product"),
                "permitted_vms": list(permitted_vms),
                "current_vm": current_vm,
            }
        }

    @property
    def device(self) -> Dict[str, Any]:
        return self._data["device"]

    @property
    def device_id(self) -> str:
        return self.device["device_id"]

    @property
    def vendor(self) -> str:
        return self.device["vendor"]

    @property
    def product(self) -> str:
        return self.device["product"]

    @property
    def permitted_vms(self) -> List[str]:
        return self.device["permitted_vms"]

    @property
    def current_vm(self) -> Optional[str]:
        return self.device["current_vm"]

    def to_json(self) -> Dict[str, Any]:
        return {
            "type": self.MSG_TYPE,
            "payload": {
                "device": {
                    "device_id": self.device_id,
                    "vendor": self.vendor,
                    "product": self.product,
                    "permitted_vms": list(self.permitted_vms),
                    "current_vm": self.current_vm,
                }
            },
        }

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "DeviceConnected":
        dev = pl.get("device")
        if not isinstance(dev, dict):
            raise ValueError("'device_connected' needs payload.device: object")
        device_id = _str_ok(dev.get("device_id"), "device_id")
        vendor = _str_ok(dev.get("vendor"), "vendor")
        product = _str_ok(dev.get("product"), "product")
        permitted = dev.get("permitted_vms", [])
        current_vm = dev.get("current_vm")
        if not _is_list_of_str(permitted):
            raise ValueError("device.permitted_vms must be list[str]")
        if current_vm is not None and not isinstance(current_vm, str):
            raise ValueError("device.current_vm must be str | None")
        return DeviceConnected(device_id, vendor, product, list(permitted), current_vm)

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["DeviceConnected"]:
        if msg.get("type") != DeviceConnected.MSG_TYPE:
            return None
        return DeviceConnected.from_payload(msg.get("payload", {}))


@dataclass
class DeviceRemoved(Message):
    MSG_TYPE = "device_removed"
    _data: Dict[str, Any]

    def __init__(self, device_id: str) -> None:
        self._data = {"device_id": _str_ok(device_id, "device_id")}

    @property
    def device_id(self) -> str:
        return self._data["device_id"]

    def to_json(self) -> Dict[str, Any]:
        return {"type": self.MSG_TYPE, "payload": {"device_id": self.device_id}}

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "DeviceRemoved":
        return DeviceRemoved(_str_ok(pl.get("device_id"), "device_id"))

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["DeviceRemoved"]:
        if msg.get("type") != DeviceRemoved.MSG_TYPE:
            return None
        return DeviceRemoved.from_payload(msg.get("payload", {}))


@dataclass
class PassthroughAck(Message):
    MSG_TYPE = "passthrough_ack"
    _data: Dict[str, Any]

    def __init__(
        self,
        device_id: str,
        current_vm: Optional[str],
        status: str,
    ) -> None:
        if current_vm is not None and not isinstance(current_vm, str):
            raise ValueError("'current_vm' must be str | None")
        if status not in ("ok", "denied", "error"):
            raise ValueError("'status' must be ok|denied|error")
        self._data = {
            "device_id": _str_ok(device_id, "device_id"),
            "current_vm": current_vm,
            "status": status,
        }

    @property
    def device_id(self) -> str:
        return self._data["device_id"]

    @property
    def current_vm(self) -> Optional[str]:
        return self._data["current_vm"]

    @property
    def status(self) -> str:
        return self._data["status"]

    def to_json(self) -> Dict[str, Any]:
        payload = {
            "device_id": self.device_id,
            "current_vm": self.current_vm,
            "status": self.status,
        }
        return {"type": self.MSG_TYPE, "payload": payload}

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "PassthroughAck":
        device_id = _str_ok(pl.get("device_id"), "device_id")
        current_vm = pl.get("current_vm")
        status = pl.get("status", "ok")
        if current_vm is not None and not isinstance(current_vm, str):
            raise ValueError("'current_vm' must be str | None")
        if status not in ("ok", "denied", "error"):
            raise ValueError("'status' must be ok|denied|error")
        return PassthroughAck(device_id, current_vm, status)

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["PassthroughAck"]:
        if msg.get("type") != PassthroughAck.MSG_TYPE:
            return None
        return PassthroughAck.from_payload(msg.get("payload", {}))


@dataclass
class ConnectedDevices(Message):
    MSG_TYPE = "connected_devices"
    _data: Dict[str, Any]

    def __init__(self, devices: Dict[str, Dict[str, Any]]) -> None:
        clean: Dict[str, Dict[str, Any]] = {}
        for did, info in devices.items():
            if not isinstance(did, str) or not isinstance(info, dict):
                raise ValueError("devices must be dict[str, object]")
            vendor = _str_ok(info.get("vendor"), "vendor")
            product = _str_ok(info.get("product"), "product")
            permitted = info.get("permitted_vms", [])
            current_vm = info.get("current_vm")
            if not _is_list_of_str(permitted):
                raise ValueError(f"devices[{did!r}].permitted_vms must be list[str]")
            if current_vm is not None and not isinstance(current_vm, str):
                raise ValueError(f"devices[{did!r}].current_vm must be str | None")
            clean[did] = {
                "vendor": vendor,
                "product": product,
                "permitted_vms": list(permitted),
                "current_vm": current_vm,
            }
        self._data = {"devices": clean}

    @property
    def devices(self) -> Dict[str, Dict[str, Any]]:
        return self._data["devices"]

    def to_json(self) -> Dict[str, Any]:
        out: Dict[str, Dict[str, Any]] = {}
        for did, info in self.devices.items():
            out[did] = {
                "vendor": info["vendor"],
                "product": info["product"],
                "permitted_vms": list(info["permitted_vms"]),
                "current_vm": info["current_vm"],
            }
        return {"type": self.MSG_TYPE, "payload": {"devices": out}}

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "ConnectedDevices":
        raw = pl.get("devices")
        if not isinstance(raw, dict):
            raise ValueError("'connected_devices' payload.devices must be dict")
        return ConnectedDevices(raw)

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["ConnectedDevices"]:
        if msg.get("type") != ConnectedDevices.MSG_TYPE:
            return None
        return ConnectedDevices.from_payload(msg.get("payload", {}))


@dataclass
class GetDevices(Message):
    MSG_TYPE = "get_devices"
    _data: Dict[str, Any]

    def __init__(self) -> None:
        self._data = {}

    def to_json(self) -> Dict[str, Any]:
        return {"type": self.MSG_TYPE, "payload": {}}

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "GetDevices":
        if pl:
            raise ValueError("'get_devices' payload must be empty {}")
        return GetDevices()

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["GetDevices"]:
        if msg.get("type") != GetDevices.MSG_TYPE:
            return None
        return GetDevices.from_payload(msg.get("payload", {}))


@dataclass
class Reset(Message):
    MSG_TYPE = "reset"
    _data: Dict[str, Any]

    def __init__(self) -> None:
        self._data = {}

    def to_json(self) -> Dict[str, Any]:
        return {"type": self.MSG_TYPE, "payload": {}}

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "Reset":
        if pl:
            raise ValueError("'reset' payload must be empty {}")
        return Reset()

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["Reset"]:
        if msg.get("type") != Reset.MSG_TYPE:
            return None
        return Reset.from_payload(msg.get("payload", {}))


@dataclass
class PassthroughRequest(Message):
    MSG_TYPE = "passthrough_request"
    _data: Dict[str, Any]

    def __init__(self, device_id: str, target_vm: str) -> None:
        self._data = {
            "device_id": _str_ok(device_id, "device_id"),
            "target_vm": _str_ok(target_vm, "target_vm"),
        }

    @property
    def device_id(self) -> str:
        return self._data["device_id"]

    @property
    def target_vm(self) -> str:
        return self._data["target_vm"]

    def to_json(self) -> Dict[str, Any]:
        return {
            "type": self.MSG_TYPE,
            "payload": {"device_id": self.device_id, "target_vm": self.target_vm},
        }

    @staticmethod
    def from_payload(pl: Dict[str, Any]) -> "PassthroughRequest":
        device_id = _str_ok(pl.get("device_id"), "device_id")
        target_vm = _str_ok(pl.get("target_vm"), "target_vm")
        return PassthroughRequest(device_id, target_vm)

    @staticmethod
    def from_message(msg: Dict[str, Any]) -> Optional["PassthroughRequest"]:
        if msg.get("type") != PassthroughRequest.MSG_TYPE:
            return None
        return PassthroughRequest.from_payload(msg.get("payload", {}))


def get_message_type(msg: Dict[str, Any]) -> Optional[str]:
    return msg.get("type")
