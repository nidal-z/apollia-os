from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.example_agent import ExampleAgent
    from ..models.required_connection_preview import RequiredConnectionPreview


T = TypeVar("T", bound="ExamplePackage")


@_attrs_define
class ExamplePackage:
    """An example as the application proposes it.

    Attributes:
        agents (list[ExampleAgent]): Its agents, each with its mission.
        description (str): What the package delivers.
        name (str): Package identifier.
        requires (list[RequiredConnectionPreview]): The connections it requires, present or missing on this machine.
        version (str): Version the binary carries.
        installed_version (None | str | Unset): Installed version, when the user installed it.
    """

    agents: list[ExampleAgent]
    description: str
    name: str
    requires: list[RequiredConnectionPreview]
    version: str
    installed_version: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        description = self.description

        name = self.name

        requires = []
        for requires_item_data in self.requires:
            requires_item = requires_item_data.to_dict()
            requires.append(requires_item)

        version = self.version

        installed_version: None | str | Unset
        if isinstance(self.installed_version, Unset):
            installed_version = UNSET
        else:
            installed_version = self.installed_version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "description": description,
                "name": name,
                "requires": requires,
                "version": version,
            }
        )
        if installed_version is not UNSET:
            field_dict["installedVersion"] = installed_version

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.example_agent import ExampleAgent
        from ..models.required_connection_preview import RequiredConnectionPreview

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = ExampleAgent.from_dict(agents_item_data)

            agents.append(agents_item)

        description = d.pop("description")

        name = d.pop("name")

        requires = []
        _requires = d.pop("requires")
        for requires_item_data in _requires:
            requires_item = RequiredConnectionPreview.from_dict(requires_item_data)

            requires.append(requires_item)

        version = d.pop("version")

        def _parse_installed_version(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        installed_version = _parse_installed_version(d.pop("installedVersion", UNSET))

        example_package = cls(
            agents=agents,
            description=description,
            name=name,
            requires=requires,
            version=version,
            installed_version=installed_version,
        )

        example_package.additional_properties = d
        return example_package

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
