from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="BatchAccess")


@_attrs_define
class BatchAccess:
    """One sensitive access of the agent a batch runs.

    Attributes:
        access_id (str): The access, as the agent names it: a tool, or `mcp:<server>`.
        family (str): Its action family: `read_personal`, `write_or_delete`, `send_external`,
            `execute_code` or `unclassified`.
        grantable (bool): Whether it can be granted for a batch. Executing code asks at every
            call, and an unclassified access has no family to grant.
        permits (str): What it lets the agent do.
    """

    access_id: str
    family: str
    grantable: bool
    permits: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        access_id = self.access_id

        family = self.family

        grantable = self.grantable

        permits = self.permits

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accessId": access_id,
                "family": family,
                "grantable": grantable,
                "permits": permits,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        access_id = d.pop("accessId")

        family = d.pop("family")

        grantable = d.pop("grantable")

        permits = d.pop("permits")

        batch_access = cls(
            access_id=access_id,
            family=family,
            grantable=grantable,
            permits=permits,
        )

        batch_access.additional_properties = d
        return batch_access

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
