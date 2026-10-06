from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.agent_package_agent import AgentPackageAgent
    from ..models.agent_package_counts import AgentPackageCounts
    from ..models.agent_package_signature import AgentPackageSignature


T = TypeVar("T", bound="AgentPackageListItem")


@_attrs_define
class AgentPackageListItem:
    """An installed package in the listing.

    Attributes:
        agent_count (int): Number of agents.
        agents (list[AgentPackageAgent]): Its agents.
        counts (AgentPackageCounts): How many items of each kind a package installed.
        description (str): Description from its manifest.
        embedded (bool): Apollia ships it inside its binary: it is never uninstalled.
        installed_at (str): When it was installed.
        name (str): Package name.
        root_missing (bool): The folder is gone from the disk.
        root_path (str): Its folder.
        signature (AgentPackageSignature): Who published an installed package, as recorded at its installation.
        version (str): Installed version.
    """

    agent_count: int
    agents: list[AgentPackageAgent]
    counts: AgentPackageCounts
    description: str
    embedded: bool
    installed_at: str
    name: str
    root_missing: bool
    root_path: str
    signature: AgentPackageSignature
    version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agent_count = self.agent_count

        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        counts = self.counts.to_dict()

        description = self.description

        embedded = self.embedded

        installed_at = self.installed_at

        name = self.name

        root_missing = self.root_missing

        root_path = self.root_path

        signature = self.signature.to_dict()

        version = self.version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_count": agent_count,
                "agents": agents,
                "counts": counts,
                "description": description,
                "embedded": embedded,
                "installed_at": installed_at,
                "name": name,
                "root_missing": root_missing,
                "root_path": root_path,
                "signature": signature,
                "version": version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_package_agent import AgentPackageAgent
        from ..models.agent_package_counts import AgentPackageCounts
        from ..models.agent_package_signature import AgentPackageSignature

        d = dict(src_dict)
        agent_count = d.pop("agent_count")

        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentPackageAgent.from_dict(agents_item_data)

            agents.append(agents_item)

        counts = AgentPackageCounts.from_dict(d.pop("counts"))

        description = d.pop("description")

        embedded = d.pop("embedded")

        installed_at = d.pop("installed_at")

        name = d.pop("name")

        root_missing = d.pop("root_missing")

        root_path = d.pop("root_path")

        signature = AgentPackageSignature.from_dict(d.pop("signature"))

        version = d.pop("version")

        agent_package_list_item = cls(
            agent_count=agent_count,
            agents=agents,
            counts=counts,
            description=description,
            embedded=embedded,
            installed_at=installed_at,
            name=name,
            root_missing=root_missing,
            root_path=root_path,
            signature=signature,
            version=version,
        )

        agent_package_list_item.additional_properties = d
        return agent_package_list_item

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
