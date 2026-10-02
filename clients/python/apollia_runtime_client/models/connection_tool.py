from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ConnectionTool")


@_attrs_define
class ConnectionTool:
    """One tool a connection brings.

    Attributes:
        family (str): Action family of the tool.
        family_source (str): Where the family comes from.
        id (str): Tool identifier as the tool registry names it (`mcp:notion/search`,
            `conn:google:ada@example.com/gmail.send`).
        prechecked (bool): Offered checked when the connection is granted to an agent.
    """

    family: str
    family_source: str
    id: str
    prechecked: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        family = self.family

        family_source = self.family_source

        id = self.id

        prechecked = self.prechecked

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "family": family,
                "familySource": family_source,
                "id": id,
                "prechecked": prechecked,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        family = d.pop("family")

        family_source = d.pop("familySource")

        id = d.pop("id")

        prechecked = d.pop("prechecked")

        connection_tool = cls(
            family=family,
            family_source=family_source,
            id=id,
            prechecked=prechecked,
        )

        connection_tool.additional_properties = d
        return connection_tool

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
