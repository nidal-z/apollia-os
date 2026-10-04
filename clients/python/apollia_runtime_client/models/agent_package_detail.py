from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_package_agent import AgentPackageAgent
    from ..models.agent_package_automation import AgentPackageAutomation
    from ..models.agent_package_content import AgentPackageContent
    from ..models.agent_package_detail_manifest import AgentPackageDetailManifest
    from ..models.agent_package_signature import AgentPackageSignature


T = TypeVar("T", bound="AgentPackageDetail")


@_attrs_define
class AgentPackageDetail:
    """An installed package, in full.

    Attributes:
        agents (list[AgentPackageAgent]): Its agents.
        author (str): Author from its manifest.
        content (AgentPackageContent): What a package installed, by kind.
        description (str): Description from its manifest.
        embedded (bool): Apollia ships it inside its binary: it is never uninstalled.
        installed_at (str): When it was installed.
        manifest (AgentPackageDetailManifest): The manifest as recorded.
        name (str): Package name.
        root_missing (bool): The folder is gone from the disk.
        root_path (str): Its folder.
        signature (AgentPackageSignature): Who published an installed package, as recorded at its installation.
        updated_at (str): When it was last written.
        version (str): Installed version.
        automations (list[AgentPackageAutomation] | Unset): The automations that start one of its agents or workflows:
            they go
            with it. Filled by the route, which holds the trigger store.
    """

    agents: list[AgentPackageAgent]
    author: str
    content: AgentPackageContent
    description: str
    embedded: bool
    installed_at: str
    manifest: AgentPackageDetailManifest
    name: str
    root_missing: bool
    root_path: str
    signature: AgentPackageSignature
    updated_at: str
    version: str
    automations: list[AgentPackageAutomation] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        author = self.author

        content = self.content.to_dict()

        description = self.description

        embedded = self.embedded

        installed_at = self.installed_at

        manifest = self.manifest.to_dict()

        name = self.name

        root_missing = self.root_missing

        root_path = self.root_path

        signature = self.signature.to_dict()

        updated_at = self.updated_at

        version = self.version

        automations: list[dict[str, Any]] | Unset = UNSET
        if not isinstance(self.automations, Unset):
            automations = []
            for automations_item_data in self.automations:
                automations_item = automations_item_data.to_dict()
                automations.append(automations_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "author": author,
                "content": content,
                "description": description,
                "embedded": embedded,
                "installed_at": installed_at,
                "manifest": manifest,
                "name": name,
                "root_missing": root_missing,
                "root_path": root_path,
                "signature": signature,
                "updated_at": updated_at,
                "version": version,
            }
        )
        if automations is not UNSET:
            field_dict["automations"] = automations

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_package_agent import AgentPackageAgent
        from ..models.agent_package_automation import AgentPackageAutomation
        from ..models.agent_package_content import AgentPackageContent
        from ..models.agent_package_detail_manifest import AgentPackageDetailManifest
        from ..models.agent_package_signature import AgentPackageSignature

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentPackageAgent.from_dict(agents_item_data)

            agents.append(agents_item)

        author = d.pop("author")

        content = AgentPackageContent.from_dict(d.pop("content"))

        description = d.pop("description")

        embedded = d.pop("embedded")

        installed_at = d.pop("installed_at")

        manifest = AgentPackageDetailManifest.from_dict(d.pop("manifest"))

        name = d.pop("name")

        root_missing = d.pop("root_missing")

        root_path = d.pop("root_path")

        signature = AgentPackageSignature.from_dict(d.pop("signature"))

        updated_at = d.pop("updated_at")

        version = d.pop("version")

        _automations = d.pop("automations", UNSET)
        automations: list[AgentPackageAutomation] | Unset = UNSET
        if _automations is not UNSET:
            automations = []
            for automations_item_data in _automations:
                automations_item = AgentPackageAutomation.from_dict(automations_item_data)

                automations.append(automations_item)

        agent_package_detail = cls(
            agents=agents,
            author=author,
            content=content,
            description=description,
            embedded=embedded,
            installed_at=installed_at,
            manifest=manifest,
            name=name,
            root_missing=root_missing,
            root_path=root_path,
            signature=signature,
            updated_at=updated_at,
            version=version,
            automations=automations,
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
