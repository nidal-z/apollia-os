from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.node_runtime_state_type_1_state import NodeRuntimeStateType1State

T = TypeVar("T", bound="NodeRuntimeStateType1")


@_attrs_define
class NodeRuntimeStateType1:
    """Being downloaded.

    Attributes:
        received (int): Bytes of the archive received so far.
        state (NodeRuntimeStateType1State):
        total (int): Size of the archive the verified manifest states; 0 until the
            manifest is verified.
    """

    received: int
    state: NodeRuntimeStateType1State
    total: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        received = self.received

        state = self.state.value

        total = self.total

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "received": received,
                "state": state,
                "total": total,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        received = d.pop("received")

        state = NodeRuntimeStateType1State(d.pop("state"))

        total = d.pop("total")

        node_runtime_state_type_1 = cls(
            received=received,
            state=state,
            total=total,
        )

        node_runtime_state_type_1.additional_properties = d
        return node_runtime_state_type_1

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
