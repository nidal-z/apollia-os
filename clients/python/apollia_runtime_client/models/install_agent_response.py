from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="InstallAgentResponse")


@_attrs_define
class InstallAgentResponse:
    """Response body for an install.

    Attributes:
        install_path (str): Install path on disk.
        name (str): Unique name of the installed agent.
        version (str): Semver version of the agent.
    """

    install_path: str
    name: str
    version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        install_path = self.install_path

        name = self.name

        version = self.version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "install_path": install_path,
                "name": name,
                "version": version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        install_path = d.pop("install_path")

        name = d.pop("name")

        version = d.pop("version")

        install_agent_response = cls(
            install_path=install_path,
            name=name,
            version=version,
        )

        install_agent_response.additional_properties = d
        return install_agent_response

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
