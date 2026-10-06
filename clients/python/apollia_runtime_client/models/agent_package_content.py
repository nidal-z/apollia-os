from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.agent_package_item import AgentPackageItem
    from ..models.agent_package_requirement import AgentPackageRequirement
    from ..models.agent_package_tool import AgentPackageTool


T = TypeVar("T", bound="AgentPackageContent")


@_attrs_define
class AgentPackageContent:
    """What a package installed, by kind.

    Attributes:
        agents (list[AgentPackageItem]): Its agents.
        connections_brought (list[str]): The connections it brought: they go with it.
        connections_required (list[AgentPackageRequirement]): The connections it requires: they stay.
        tools (list[AgentPackageTool]): Its Python tools.
        workflows (list[AgentPackageItem]): Its workflows.
    """

    agents: list[AgentPackageItem]
    connections_brought: list[str]
    connections_required: list[AgentPackageRequirement]
    tools: list[AgentPackageTool]
    workflows: list[AgentPackageItem]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = []
        for agents_item_data in self.agents:
            agents_item = agents_item_data.to_dict()
            agents.append(agents_item)

        connections_brought = self.connections_brought

        connections_required = []
        for connections_required_item_data in self.connections_required:
            connections_required_item = connections_required_item_data.to_dict()
            connections_required.append(connections_required_item)

        tools = []
        for tools_item_data in self.tools:
            tools_item = tools_item_data.to_dict()
            tools.append(tools_item)

        workflows = []
        for workflows_item_data in self.workflows:
            workflows_item = workflows_item_data.to_dict()
            workflows.append(workflows_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "connections_brought": connections_brought,
                "connections_required": connections_required,
                "tools": tools,
                "workflows": workflows,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_package_item import AgentPackageItem
        from ..models.agent_package_requirement import AgentPackageRequirement
        from ..models.agent_package_tool import AgentPackageTool

        d = dict(src_dict)
        agents = []
        _agents = d.pop("agents")
        for agents_item_data in _agents:
            agents_item = AgentPackageItem.from_dict(agents_item_data)

            agents.append(agents_item)

        connections_brought = cast(list[str], d.pop("connections_brought"))

        connections_required = []
        _connections_required = d.pop("connections_required")
        for connections_required_item_data in _connections_required:
            connections_required_item = AgentPackageRequirement.from_dict(
                connections_required_item_data
            )

            connections_required.append(connections_required_item)

        tools = []
        _tools = d.pop("tools")
        for tools_item_data in _tools:
            tools_item = AgentPackageTool.from_dict(tools_item_data)

            tools.append(tools_item)

        workflows = []
        _workflows = d.pop("workflows")
        for workflows_item_data in _workflows:
            workflows_item = AgentPackageItem.from_dict(workflows_item_data)

            workflows.append(workflows_item)

        agent_package_content = cls(
            agents=agents,
            connections_brought=connections_brought,
            connections_required=connections_required,
            tools=tools,
            workflows=workflows,
        )

        agent_package_content.additional_properties = d
        return agent_package_content

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
