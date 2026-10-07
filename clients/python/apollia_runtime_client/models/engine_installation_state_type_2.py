from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_installation_kind import EngineInstallationKind
from ..models.engine_installation_state_type_2_state import EngineInstallationStateType2State

T = TypeVar("T", bound="EngineInstallationStateType2")


@_attrs_define
class EngineInstallationStateType2:
    """The signature held and the archive is arriving.

    Attributes:
        kind (EngineInstallationKind): What an installation is, for the one who asked.
        received (int): Bytes of the archive received so far.
        state (EngineInstallationStateType2State):
        total (int): Size of the archive the verified manifest states.
    """

    kind: EngineInstallationKind
    received: int
    state: EngineInstallationStateType2State
    total: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        kind = self.kind.value

        received = self.received

        state = self.state.value

        total = self.total

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "kind": kind,
                "received": received,
                "state": state,
                "total": total,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        kind = EngineInstallationKind(d.pop("kind"))

        received = d.pop("received")

        state = EngineInstallationStateType2State(d.pop("state"))

        total = d.pop("total")

        engine_installation_state_type_2 = cls(
            kind=kind,
            received=received,
            state=state,
            total=total,
        )

        engine_installation_state_type_2.additional_properties = d
        return engine_installation_state_type_2

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
