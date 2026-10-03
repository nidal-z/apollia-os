from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_package_agent import AgentPackageAgent
    from ..models.agent_package_trigger import AgentPackageTrigger


T = TypeVar("T", bound="AgentPackagePreview")


@_attrs_define
class AgentPackagePreview:
    """What a package folder contains, read without writing anything.

    Attributes:
        agents (list[AgentPackageAgent]): Its agents; empty when the manifest is invalid.
        author (str): Author.
        description (str): Description.
        name (str): Package name.
        pip_packages (list[str]): The Python dependencies it installs, every agent together.
        trigger_count (int): Number of triggers.
        triggers (list[AgentPackageTrigger]): Its triggers; empty when the manifest is invalid.
        valid (bool): The manifest holds its invariants.
        version (str): Version.
        error (None | str | Unset): Why it does not, when it does not.
    """

    agents: list[AgentPackageAgent]
    author: str
    description: str
    name: str
    pip_packages: list[str]
    trigger_count: int
    triggers: list[AgentPackageTrigger]
    valid: bool
    version: str
    error: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        author = self.author

        description = self.description

        name = self.name

        pip_packages = self.pip_packages

        trigger_count = self.trigger_count

        triggers = []
        for triggers_item_data in self.triggers:
            triggers_item = triggers_item_data.to_dict()
            triggers.append(triggers_item)

        valid = self.valid

        version = self.version

        error: None | str | Unset
        if isinstance(self.error, Unset):
            error = UNSET
        else:
            error = self.error

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "author": author,
                "description": description,
                "name": name,
                "pip_packages": pip_packages,
                "trigger_count": trigger_count,
                "triggers": triggers,
                "valid": valid,
                "version": version,
            }
        )
        if error is not UNSET:
            field_dict["error"] = error

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_package_agent import AgentPackageAgent
        from ..models.agent_package_trigger import AgentPackageTrigger

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentPackageAgent.from_dict(agents_item_data)

            agents.append(agents_item)

        author = d.pop("author")

        description = d.pop("description")

        name = d.pop("name")

        pip_packages = cast(list[str], d.pop("pip_packages"))

        trigger_count = d.pop("trigger_count")

        triggers = []
        _triggers = d.pop("triggers")
        for triggers_item_data in _triggers:
            triggers_item = AgentPackageTrigger.from_dict(triggers_item_data)

            triggers.append(triggers_item)

        valid = d.pop("valid")

        version = d.pop("version")

        def _parse_error(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        error = _parse_error(d.pop("error", UNSET))

        agent_package_preview = cls(
            agents=agents,
            author=author,
            description=description,
            name=name,
            pip_packages=pip_packages,
            trigger_count=trigger_count,
            triggers=triggers,
            valid=valid,
            version=version,
            error=error,
        )

        agent_package_preview.additional_properties = d
        return agent_package_preview

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
