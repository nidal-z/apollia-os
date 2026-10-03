from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.node_runtime_state_type_3_state import NodeRuntimeStateType3State

T = TypeVar("T", bound="NodeRuntimeStateType3")


@_attrs_define
class NodeRuntimeStateType3:
    """The last download failed; asking for Node.js again retries it.

    Attributes:
        error (str): Why, readable as is.
        state (NodeRuntimeStateType3State):
    """

    error: str
    state: NodeRuntimeStateType3State
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        error = self.error

        state = self.state.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "error": error,
                "state": state,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        error = d.pop("error")

        state = NodeRuntimeStateType3State(d.pop("state"))

        node_runtime_state_type_3 = cls(
            error=error,
            state=state,
        )

        node_runtime_state_type_3.additional_properties = d
        return node_runtime_state_type_3

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
