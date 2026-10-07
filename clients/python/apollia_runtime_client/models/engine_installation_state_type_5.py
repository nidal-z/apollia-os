from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_installation_kind import EngineInstallationKind
from ..models.engine_installation_state_type_5_state import EngineInstallationStateType5State
from ..models.engine_refusal_code import EngineRefusalCode

T = TypeVar("T", bound="EngineInstallationStateType5")


@_attrs_define
class EngineInstallationStateType5:
    """Refused or failed. Nothing was installed.

    Attributes:
        archive_requested (bool): True when an archive had been requested from the mirror: false
            means the refusal came before any download.
        code (EngineRefusalCode): Why the store refused, as a stable word an interface can branch on. The
            sentence of the error says the same to a reader.
        error (str): Why, readable as is.
        kind (EngineInstallationKind): What an installation is, for the one who asked.
        state (EngineInstallationStateType5State):
    """

    archive_requested: bool
    code: EngineRefusalCode
    error: str
    kind: EngineInstallationKind
    state: EngineInstallationStateType5State
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        archive_requested = self.archive_requested

        code = self.code.value

        error = self.error

        kind = self.kind.value

        state = self.state.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "archive_requested": archive_requested,
                "code": code,
                "error": error,
                "kind": kind,
                "state": state,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        archive_requested = d.pop("archive_requested")

        code = EngineRefusalCode(d.pop("code"))

        error = d.pop("error")

        kind = EngineInstallationKind(d.pop("kind"))

        state = EngineInstallationStateType5State(d.pop("state"))

        engine_installation_state_type_5 = cls(
            archive_requested=archive_requested,
            code=code,
            error=error,
            kind=kind,
            state=state,
        )

        engine_installation_state_type_5.additional_properties = d
        return engine_installation_state_type_5

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
