from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ToolStatEntry")


@_attrs_define
class ToolStatEntry:
    """Per-tool aggregated statistics within a session.

    Attributes:
        calls (int): Number of invocations (any status).
        executed (int): Number of invocations that ended with `Executed` status.
        refused (int): Number of invocations that ended with `Refused` status.
        tool_name (str): Tool name (scoped, e.g. `"bash"` or `"native:read_file"`).
    """

    calls: int
    executed: int
    refused: int
    tool_name: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        calls = self.calls

        executed = self.executed

        refused = self.refused

        tool_name = self.tool_name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "calls": calls,
                "executed": executed,
                "refused": refused,
                "tool_name": tool_name,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        calls = d.pop("calls")

        executed = d.pop("executed")

        refused = d.pop("refused")

        tool_name = d.pop("tool_name")

        tool_stat_entry = cls(
            calls=calls,
            executed=executed,
            refused=refused,
            tool_name=tool_name,
        )

        tool_stat_entry.additional_properties = d
        return tool_stat_entry

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
