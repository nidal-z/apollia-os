from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="UpdateAgentResponse")


@_attrs_define
class UpdateAgentResponse:
    """Response to an agent update.

    Distinct from the install answer because an update has an outcome an
    install cannot have: a runtime instance that had to be cycled.

        Attributes:
            install_path (str): Install path on disk.
            name (str): Unique name of the updated agent.
            restart_outcome (str): One of the [`restart_outcome`] constants.
            version (str): Semver version read from the new module's manifest.
            restart_error (None | str | Unset): Raw cause when the stop or the start failed, `None` otherwise.
    """

    install_path: str
    name: str
    restart_outcome: str
    version: str
    restart_error: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        install_path = self.install_path

        name = self.name

        restart_outcome = self.restart_outcome

        version = self.version

        restart_error: None | str | Unset
        if isinstance(self.restart_error, Unset):
            restart_error = UNSET
        else:
            restart_error = self.restart_error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "install_path": install_path,
                "name": name,
                "restart_outcome": restart_outcome,
                "version": version,
            }
        )
        if restart_error is not UNSET:
            field_dict["restart_error"] = restart_error

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        install_path = d.pop("install_path")

        name = d.pop("name")

        restart_outcome = d.pop("restart_outcome")

        version = d.pop("version")

        def _parse_restart_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        restart_error = _parse_restart_error(d.pop("restart_error", UNSET))

        update_agent_response = cls(
            install_path=install_path,
            name=name,
            restart_outcome=restart_outcome,
            version=version,
            restart_error=restart_error,
        )

        update_agent_response.additional_properties = d
        return update_agent_response

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
