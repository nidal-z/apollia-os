from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.node_runtime_state_type_0_state import NodeRuntimeStateType0State

T = TypeVar("T", bound="NodeRuntimeStateType0")


@_attrs_define
class NodeRuntimeStateType0:
    """Not installed, and no download running.

    Attributes:
        state (NodeRuntimeStateType0State):
    """

    state: NodeRuntimeStateType0State
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        state = self.state.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "state": state,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        state = NodeRuntimeStateType0State(d.pop("state"))

        node_runtime_state_type_0 = cls(
            state=state,
        )

        node_runtime_state_type_0.additional_properties = d
        return node_runtime_state_type_0

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
