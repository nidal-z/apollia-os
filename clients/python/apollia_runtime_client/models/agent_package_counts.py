from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="AgentPackageCounts")


@_attrs_define
class AgentPackageCounts:
    """How many items of each kind a package installed.

    Attributes:
        agents (int): Agents.
        connections (int): Connections, brought and required together.
        tools (int): Python tools.
        workflows (int): Workflows.
    """

    agents: int
    connections: int
    tools: int
    workflows: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = self.agents

        connections = self.connections

        tools = self.tools

        workflows = self.workflows

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "connections": connections,
                "tools": tools,
                "workflows": workflows,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agents = d.pop("agents")

        connections = d.pop("connections")

        tools = d.pop("tools")

        workflows = d.pop("workflows")

        agent_package_counts = cls(
            agents=agents,
            connections=connections,
            tools=tools,
            workflows=workflows,
        )

        agent_package_counts.additional_properties = d
        return agent_package_counts

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
