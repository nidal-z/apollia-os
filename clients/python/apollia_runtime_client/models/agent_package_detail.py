from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.agent_package_agent import AgentPackageAgent
    from ..models.agent_package_detail_manifest import AgentPackageDetailManifest


T = TypeVar("T", bound="AgentPackageDetail")


@_attrs_define
class AgentPackageDetail:
    """An installed package, in full.

    Attributes:
        agents (list[AgentPackageAgent]): Its agents.
        author (str): Author from its manifest.
        description (str): Description from its manifest.
        installed_at (str): When it was installed.
        manifest (AgentPackageDetailManifest): The manifest as recorded.
        name (str): Package name.
        root_missing (bool): The folder is gone from the disk.
        root_path (str): Its folder.
        updated_at (str): When it was last written.
        version (str): Installed version.
    """

    agents: list[AgentPackageAgent]
    author: str
    description: str
    installed_at: str
    manifest: AgentPackageDetailManifest
    name: str
    root_missing: bool
    root_path: str
    updated_at: str
    version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        author = self.author

        description = self.description

        installed_at = self.installed_at

        manifest = self.manifest.to_dict()

        name = self.name

        root_missing = self.root_missing

        root_path = self.root_path

        updated_at = self.updated_at

        version = self.version

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "author": author,
                "description": description,
                "installed_at": installed_at,
                "manifest": manifest,
                "name": name,
                "root_missing": root_missing,
                "root_path": root_path,
                "updated_at": updated_at,
                "version": version,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_package_agent import AgentPackageAgent
        from ..models.agent_package_detail_manifest import AgentPackageDetailManifest

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentPackageAgent.from_dict(agents_item_data)

            agents.append(agents_item)

        author = d.pop("author")

        description = d.pop("description")

        installed_at = d.pop("installed_at")

        manifest = AgentPackageDetailManifest.from_dict(d.pop("manifest"))

        name = d.pop("name")

        root_missing = d.pop("root_missing")

        root_path = d.pop("root_path")

        updated_at = d.pop("updated_at")

        version = d.pop("version")

        agent_package_detail = cls(
            agents=agents,
            author=author,
            description=description,
            installed_at=installed_at,
            manifest=manifest,
            name=name,
            root_missing=root_missing,
            root_path=root_path,
            updated_at=updated_at,
            version=version,
        )

        agent_package_detail.additional_properties = d
        return agent_package_detail

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
