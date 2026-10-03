from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="AddedAccess")


@_attrs_define
class AddedAccess:
    """An access the update grants that the installed version did not.

    Attributes:
        access_id (str): Identifier of the access.
        agent_id (str): Agent that gains it.
        permits (str): What it lets the agent do.
        sensitive (bool): Using it is sensitive.
    """

    access_id: str
    agent_id: str
    permits: str
    sensitive: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        access_id = self.access_id

        agent_id = self.agent_id

        permits = self.permits

        sensitive = self.sensitive

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "accessId": access_id,
                "agentId": agent_id,
                "permits": permits,
                "sensitive": sensitive,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        access_id = d.pop("accessId")

        agent_id = d.pop("agentId")

        permits = d.pop("permits")

        sensitive = d.pop("sensitive")

        added_access = cls(
            access_id=access_id,
            agent_id=agent_id,
            permits=permits,
            sensitive=sensitive,
        )

        added_access.additional_properties = d
        return added_access

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
