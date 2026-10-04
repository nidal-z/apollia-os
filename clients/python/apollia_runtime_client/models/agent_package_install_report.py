from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="AgentPackageInstallReport")


@_attrs_define
class AgentPackageInstallReport:
    """What an installation did.

    Attributes:
        agent_count (int): Agents recorded.
        name (str): Package name.
        trigger_count (int): Triggers written.
        trigger_errors (list[str]): Triggers refused, one line each.
        version (str): Installed version.
    """

    agent_count: int
    name: str
    trigger_count: int
    trigger_errors: list[str]
    version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent_count = self.agent_count

        name = self.name

        trigger_count = self.trigger_count

        trigger_errors = self.trigger_errors

        version = self.version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_count": agent_count,
                "name": name,
                "trigger_count": trigger_count,
                "trigger_errors": trigger_errors,
                "version": version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agent_count = d.pop("agent_count")

        name = d.pop("name")

        trigger_count = d.pop("trigger_count")

        trigger_errors = cast(list[str], d.pop("trigger_errors"))

        version = d.pop("version")

        agent_package_install_report = cls(
            agent_count=agent_count,
            name=name,
            trigger_count=trigger_count,
            trigger_errors=trigger_errors,
            version=version,
        )

        agent_package_install_report.additional_properties = d
        return agent_package_install_report

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
