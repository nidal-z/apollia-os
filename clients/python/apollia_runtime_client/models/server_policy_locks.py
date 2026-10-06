from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ServerPolicyLocks")


@_attrs_define
class ServerPolicyLocks:
    """Which settings of the compatible server the organisation policy imposes.
    An imposed setting cannot be written: the value in force is the policy's.

        Attributes:
            allow_request_mcp (bool): `server.allow_request_mcp`: the policy refuses the MCP servers a
                request brings.
            require_auth (bool): `server.require_auth`: the policy requires a token on every request.
            serve_on_lan (bool): `server.serve_on_lan`: the policy keeps the server on the machine.
    """

    allow_request_mcp: bool
    require_auth: bool
    serve_on_lan: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        allow_request_mcp = self.allow_request_mcp

        require_auth = self.require_auth

        serve_on_lan = self.serve_on_lan

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "allowRequestMcp": allow_request_mcp,
                "requireAuth": require_auth,
                "serveOnLan": serve_on_lan,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        allow_request_mcp = d.pop("allowRequestMcp")

        require_auth = d.pop("requireAuth")

        serve_on_lan = d.pop("serveOnLan")

        server_policy_locks = cls(
            allow_request_mcp=allow_request_mcp,
            require_auth=require_auth,
            serve_on_lan=serve_on_lan,
        )

        server_policy_locks.additional_properties = d
        return server_policy_locks

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
