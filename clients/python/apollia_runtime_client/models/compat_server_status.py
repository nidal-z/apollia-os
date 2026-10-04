from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CompatServerStatus")


@_attrs_define
class CompatServerStatus:
    """What the compatible server is doing.

    Attributes:
        active_tokens (int): How many tokens of the server are in use (not revoked).
        address (str): The address a client copies: `http://127.0.0.1:<port>` on the machine
            alone, `http://<IPv4 address on the local network>:<port>` otherwise.
        port (int): The port it listens on, or the configured port when it is stopped.
        running (bool): Whether the listener is open.
        serve_on_lan (bool): Whether it listens on every interface, reachable from the local
            network, or the configured choice when it is stopped.
        last_error (None | str | Unset): Why the last start failed, cleared by the next successful start.
    """

    active_tokens: int
    address: str
    port: int
    running: bool
    serve_on_lan: bool
    last_error: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        active_tokens = self.active_tokens

        address = self.address

        port = self.port

        running = self.running

        serve_on_lan = self.serve_on_lan

        last_error: None | str | Unset
        if isinstance(self.last_error, Unset):
            last_error = UNSET
        else:
            last_error = self.last_error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "activeTokens": active_tokens,
                "address": address,
                "port": port,
                "running": running,
                "serveOnLan": serve_on_lan,
            }
        )
        if last_error is not UNSET:
            field_dict["lastError"] = last_error

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        active_tokens = d.pop("activeTokens")

        address = d.pop("address")

        port = d.pop("port")

        running = d.pop("running")

        serve_on_lan = d.pop("serveOnLan")

        def _parse_last_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        last_error = _parse_last_error(d.pop("lastError", UNSET))

        compat_server_status = cls(
            active_tokens=active_tokens,
            address=address,
            port=port,
            running=running,
            serve_on_lan=serve_on_lan,
            last_error=last_error,
        )

        compat_server_status.additional_properties = d
        return compat_server_status

    @property
    def additional_keys(self) -> list[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
