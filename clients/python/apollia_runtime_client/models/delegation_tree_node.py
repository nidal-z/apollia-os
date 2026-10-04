from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="DelegationTreeNode")


@_attrs_define
class DelegationTreeNode:
    """One node of the delegation tree.

    Attributes:
        agent_id (str): Agent identifier (runtime UUID, or name if resolution fails).
        agent_name (str): Human-readable agent name.
        children (list[DelegationTreeNode]): Child delegations (A2A subtasks).
        status (str): Current status: `submitted`, `working`, `completed`, `failed`, `running`, etc.
        started_at (None | str | Unset): ISO 8601 start timestamp (None for the root if unknown).
    """

    agent_id: str
    agent_name: str
    children: list[DelegationTreeNode]
    status: str
    started_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent_id = self.agent_id

        agent_name = self.agent_name

        children = []
        for children_item_data in self.children:
            children_item = children_item_data.to_dict()
            children.append(children_item)

        status = self.status

        started_at: None | str | Unset
        if isinstance(self.started_at, Unset):
            started_at = UNSET
        else:
            started_at = self.started_at

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_id": agent_id,
                "agent_name": agent_name,
                "children": children,
                "status": status,
            }
        )
        if started_at is not UNSET:
            field_dict["started_at"] = started_at

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        agent_name = d.pop("agent_name")

        children = []
        _children = d.pop("children")
        for children_item_data in _children:
            children_item = DelegationTreeNode.from_dict(children_item_data)

            children.append(children_item)

        status = d.pop("status")

        def _parse_started_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        started_at = _parse_started_at(d.pop("started_at", UNSET))

        delegation_tree_node = cls(
            agent_id=agent_id,
            agent_name=agent_name,
            children=children,
            status=status,
            started_at=started_at,
        )

        delegation_tree_node.additional_properties = d
        return delegation_tree_node

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
