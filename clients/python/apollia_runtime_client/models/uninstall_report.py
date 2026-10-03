from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="UninstallReport")


@_attrs_define
class UninstallReport:
    """What an uninstallation removed, and what it kept.

    Attributes:
        agents (list[str]): Agents removed.
        batches_removed (int): Batches erased with the history.
        connections_kept (list[str]): Connections the package required, kept.
        history_cleared (bool): The history was erased.
        leftovers (list[str]): What could not be removed, and why. The uninstallation goes on past
            such a step, so that the package can be installed again.
        mcp_servers_removed (list[str]): MCP servers the package brought, removed.
        memory_cleared (bool): The memory of the agents was erased.
        name (str): The package, or the agent uninstalled on its own.
        tasks_removed (int): Tasks erased with the history.
        tools (list[str]): Python tools removed.
        triggers_removed (list[str]): Triggers removed.
        version (str): The version removed.
        workflow_runs_removed (int): Workflow runs erased with the history.
        workflows (list[str]): Workflows removed.
    """

    agents: list[str]
    batches_removed: int
    connections_kept: list[str]
    history_cleared: bool
    leftovers: list[str]
    mcp_servers_removed: list[str]
    memory_cleared: bool
    name: str
    tasks_removed: int
    tools: list[str]
    triggers_removed: list[str]
    version: str
    workflow_runs_removed: int
    workflows: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        agents = self.agents

        batches_removed = self.batches_removed

        connections_kept = self.connections_kept

        history_cleared = self.history_cleared

        leftovers = self.leftovers

        mcp_servers_removed = self.mcp_servers_removed

        memory_cleared = self.memory_cleared

        name = self.name

        tasks_removed = self.tasks_removed

        tools = self.tools

        triggers_removed = self.triggers_removed

        version = self.version

        workflow_runs_removed = self.workflow_runs_removed

        workflows = self.workflows

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agents": agents,
                "batchesRemoved": batches_removed,
                "connectionsKept": connections_kept,
                "historyCleared": history_cleared,
                "leftovers": leftovers,
                "mcpServersRemoved": mcp_servers_removed,
                "memoryCleared": memory_cleared,
                "name": name,
                "tasksRemoved": tasks_removed,
                "tools": tools,
                "triggersRemoved": triggers_removed,
                "version": version,
                "workflowRunsRemoved": workflow_runs_removed,
                "workflows": workflows,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        agents = cast(list[str], d.pop("agents"))

        batches_removed = d.pop("batchesRemoved")

        connections_kept = cast(list[str], d.pop("connectionsKept"))

        history_cleared = d.pop("historyCleared")

        leftovers = cast(list[str], d.pop("leftovers"))

        mcp_servers_removed = cast(list[str], d.pop("mcpServersRemoved"))

        memory_cleared = d.pop("memoryCleared")

        name = d.pop("name")

        tasks_removed = d.pop("tasksRemoved")

        tools = cast(list[str], d.pop("tools"))

        triggers_removed = cast(list[str], d.pop("triggersRemoved"))

        version = d.pop("version")

        workflow_runs_removed = d.pop("workflowRunsRemoved")

        workflows = cast(list[str], d.pop("workflows"))

        uninstall_report = cls(
            agents=agents,
            batches_removed=batches_removed,
            connections_kept=connections_kept,
            history_cleared=history_cleared,
            leftovers=leftovers,
            mcp_servers_removed=mcp_servers_removed,
            memory_cleared=memory_cleared,
            name=name,
            tasks_removed=tasks_removed,
            tools=tools,
            triggers_removed=triggers_removed,
            version=version,
            workflow_runs_removed=workflow_runs_removed,
            workflows=workflows,
        )

        uninstall_report.additional_properties = d
        return uninstall_report

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
