from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="PackageErrorBody")


@_attrs_define
class PackageErrorBody:
    """Error body of the package routes: the message, and a stable code a client
    can act on.

        Attributes:
            code (str): Stable code: `archive`, `invalid_signature`, `signature_required`,
                `downgrade`, `agent_taken`, `deps_confirmation_required`,
                `changes_confirmation_required`, `mcp_unavailable`, `mcp_server`,
                `environment`, `invalid_key`, `not_found`, `journal_unavailable`,
                `internal`.
            error (str): Human-readable description.
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

        package_error_body = cls(
            code=code,
            error=error,
        )

        package_error_body.additional_properties = d
        return package_error_body

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
