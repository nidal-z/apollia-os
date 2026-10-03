from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.node_runtime_state_type_2_state import NodeRuntimeStateType2State

T = TypeVar("T", bound="NodeRuntimeStateType2")


@_attrs_define
class NodeRuntimeStateType2:
    """Installed and verified.

    Attributes:
        path (str): Directory it was extracted into.
        state (NodeRuntimeStateType2State):
        tag (str): Release tag, `v22.11.0`.
    """

    path: str
    state: NodeRuntimeStateType2State
    tag: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        path = self.path

        state = self.state.value

        tag = self.tag

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "path": path,
                "state": state,
                "tag": tag,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        path = d.pop("path")

        state = NodeRuntimeStateType2State(d.pop("state"))

        tag = d.pop("tag")

        node_runtime_state_type_2 = cls(
            path=path,
            state=state,
            tag=tag,
        )

        node_runtime_state_type_2.additional_properties = d
        return node_runtime_state_type_2

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
