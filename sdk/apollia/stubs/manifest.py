"""Type stubs for AgentManifest returned by agent.manifest()."""

from __future__ import annotations

from typing import Optional
from typing_extensions import TypedDict


class StepBudgetDict(TypedDict, total=False):
    max_steps: int
    max_tool_calls: int
    wall_clock_secs: int


class AgentSkillDict(TypedDict):
    id: str
    name: str
    description: str
    input_modes: list[str]
    output_modes: list[str]


class AgentManifestDict(TypedDict, total=False):
    name: str
    version: str
    description: str
    tools_required: list[str]
    tools_optional: list[str]
    supports_streaming: bool
    supports_a2a: bool
    memory_namespace: Optional[str]
    shared_memory_namespaces: list[str]
    max_concurrent_tasks: int
    step_budget: Optional[StepBudgetDict]
    network_allowlist: Optional[list[str]]
    dangerous_tools_allowed: bool
    tags: list[str]
    skills: list[AgentSkillDict]
    execution_mode: str
    system_prompt: Optional[str]
    tools_requiring_approval: list[str]
    llm_backend: Optional[str]
    packages: list[str]  # pip requirements, standard syntax, default []
    agent_type: str | None
    examples: list[str]
    limitations: list[str]
    setup_notes: str | None
