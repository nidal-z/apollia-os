from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="AgentPackageAgent")


@_attrs_define
class AgentPackageAgent:
    """An agent of a package.

    Attributes:
        entry (str): Entry point, relative to the package folder.
        name (str): Agent name.
        role (str): `director`, `worker` or `assistant`.
    """

    entry: str
    name: str
    role: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        entry = self.entry

        name = self.name

        role = self.role

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "entry": entry,
                "name": name,
                "role": role,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        entry = d.pop("entry")

        name = d.pop("name")

        role = d.pop("role")

        agent_package_agent = cls(
            entry=entry,
            name=name,
            role=role,
        )

        agent_package_agent.additional_properties = d
        return agent_package_agent

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
