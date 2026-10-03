from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="McpBridgeErrorBody")


@_attrs_define
class McpBridgeErrorBody:
    """Error body of the MCP bridge routes.

    Attributes:
        code (str): Stable code: `bridge_disabled`, `unknown_tool`, `invalid_arguments`,
            `invalid_kind`, `unknown_item`, `unavailable` or `internal`.
        error (str): What went wrong, in words a person can act on.
    """

    code: str
    error: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        code = self.code

        error = self.error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "code": code,
                "error": error,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        code = d.pop("code")

        error = d.pop("error")

        mcp_bridge_error_body = cls(
            code=code,
            error=error,
        )

        mcp_bridge_error_body.additional_properties = d
        return mcp_bridge_error_body

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
