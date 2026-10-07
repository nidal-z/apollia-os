from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_installation_state_type_0_state import EngineInstallationStateType0State

T = TypeVar("T", bound="EngineInstallationStateType0")


@_attrs_define
class EngineInstallationStateType0:
    """Nothing running, and no outcome waiting to be read.

    Attributes:
        state (EngineInstallationStateType0State):
    """

    state: EngineInstallationStateType0State
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
        state = EngineInstallationStateType0State(d.pop("state"))

        engine_installation_state_type_0 = cls(
            state=state,
        )

        engine_installation_state_type_0.additional_properties = d
        return engine_installation_state_type_0

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
