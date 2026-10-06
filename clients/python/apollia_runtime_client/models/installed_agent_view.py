from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.agent_skill_view import AgentSkillView


T = TypeVar("T", bound="InstalledAgentView")


@_attrs_define
class InstalledAgentView:
    """One agent of the machine: stored, registered, or both.

    Attributes:
        enabled (bool): Whether the agent is made available at boot.
        examples (list[str]): Prompt examples of typical uses.
        limitations (list[str]): What the agent does not do.
        name (str): Unique agent name.
        shared_memory_namespaces (list[str]): Shared memory namespaces the agent reads.
        skills (list[AgentSkillView]): Declared skills.
        supports_a2a (bool): Whether the agent takes part in agent-to-agent calls.
        tags (list[str]): Free-form tags for routing and discovery.
        version (str): Semver version.
        agent_class (None | str | Unset): Name of the Python class of the agent.
        agent_type (None | str | Unset): Semantic role: `"worker"`, `"assistant"`, `"system"`, or `None`.
        description (None | str | Unset): What the agent is for.
        execution_mode (None | str | Unset): Execution mode (`"auto"`, `"direct"`, `"orchestrated"`).
        id (None | str | Unset): Runtime identifier, `None` when stored but not registered.
        installed_at (None | str | Unset): Install timestamp (RFC 3339), `None` for an agent registered without
            being stored.
        memory_namespace (None | str | Unset): Primary memory namespace, as the manifest declares it.
        runtime_status (None | str | Unset): Runtime state (`"active"`, `"degraded"`, `"stopped"`...), `None` when
            not started.
        setup_notes (None | str | Unset): Configuration needed before the first use.
    """

    enabled: bool
    examples: list[str]
    limitations: list[str]
    name: str
    shared_memory_namespaces: list[str]
    skills: list[AgentSkillView]
    supports_a2a: bool
    tags: list[str]
    version: str
    agent_class: None | str | Unset = UNSET
    agent_type: None | str | Unset = UNSET
    description: None | str | Unset = UNSET
    execution_mode: None | str | Unset = UNSET
    id: None | str | Unset = UNSET
    installed_at: None | str | Unset = UNSET
    memory_namespace: None | str | Unset = UNSET
    runtime_status: None | str | Unset = UNSET
    setup_notes: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        enabled = self.enabled

        examples = self.examples

        limitations = self.limitations

        name = self.name

        shared_memory_namespaces = self.shared_memory_namespaces

        skills = []
        for skills_item_data in self.skills:
            skills_item = skills_item_data.to_dict()
            skills.append(skills_item)

        supports_a2a = self.supports_a2a

        tags = self.tags

        version = self.version

        agent_class: None | str | Unset
        if isinstance(self.agent_class, Unset):
            agent_class = UNSET
        else:
            agent_class = self.agent_class

        agent_type: None | str | Unset
        if isinstance(self.agent_type, Unset):
            agent_type = UNSET
        else:
            agent_type = self.agent_type

        description: None | str | Unset
        if isinstance(self.description, Unset):
            description = UNSET
        else:
            description = self.description

        execution_mode: None | str | Unset
        if isinstance(self.execution_mode, Unset):
            execution_mode = UNSET
        else:
            execution_mode = self.execution_mode

        id: None | str | Unset
        if isinstance(self.id, Unset):
            id = UNSET
        else:
            id = self.id

        installed_at: None | str | Unset
        if isinstance(self.installed_at, Unset):
            installed_at = UNSET
        else:
            installed_at = self.installed_at

        memory_namespace: None | str | Unset
        if isinstance(self.memory_namespace, Unset):
            memory_namespace = UNSET
        else:
            memory_namespace = self.memory_namespace

        runtime_status: None | str | Unset
        if isinstance(self.runtime_status, Unset):
            runtime_status = UNSET
        else:
            runtime_status = self.runtime_status

        setup_notes: None | str | Unset
        if isinstance(self.setup_notes, Unset):
            setup_notes = UNSET
        else:
            setup_notes = self.setup_notes

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "enabled": enabled,
                "examples": examples,
                "limitations": limitations,
                "name": name,
                "shared_memory_namespaces": shared_memory_namespaces,
                "skills": skills,
                "supports_a2a": supports_a2a,
                "tags": tags,
                "version": version,
            }
        )
        if agent_class is not UNSET:
            field_dict["agent_class"] = agent_class
        if agent_type is not UNSET:
            field_dict["agent_type"] = agent_type
        if description is not UNSET:
            field_dict["description"] = description
        if execution_mode is not UNSET:
            field_dict["execution_mode"] = execution_mode
        if id is not UNSET:
            field_dict["id"] = id
        if installed_at is not UNSET:
            field_dict["installed_at"] = installed_at
        if memory_namespace is not UNSET:
            field_dict["memory_namespace"] = memory_namespace
        if runtime_status is not UNSET:
            field_dict["runtime_status"] = runtime_status
        if setup_notes is not UNSET:
            field_dict["setup_notes"] = setup_notes

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.agent_skill_view import AgentSkillView

        d = dict(src_dict)
        enabled = d.pop("enabled")

        examples = cast(list[str], d.pop("examples"))

        limitations = cast(list[str], d.pop("limitations"))

        name = d.pop("name")

        shared_memory_namespaces = cast(list[str], d.pop("shared_memory_namespaces"))

        skills = []
        _skills = d.pop("skills")
        for skills_item_data in _skills:
            skills_item = AgentSkillView.from_dict(skills_item_data)

            skills.append(skills_item)

        supports_a2a = d.pop("supports_a2a")

        tags = cast(list[str], d.pop("tags"))

        version = d.pop("version")

        def _parse_agent_class(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent_class = _parse_agent_class(d.pop("agent_class", UNSET))

        def _parse_agent_type(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent_type = _parse_agent_type(d.pop("agent_type", UNSET))

        def _parse_description(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        description = _parse_description(d.pop("description", UNSET))

        def _parse_execution_mode(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        execution_mode = _parse_execution_mode(d.pop("execution_mode", UNSET))

        def _parse_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        id = _parse_id(d.pop("id", UNSET))

        def _parse_installed_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        installed_at = _parse_installed_at(d.pop("installed_at", UNSET))

        def _parse_memory_namespace(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        memory_namespace = _parse_memory_namespace(d.pop("memory_namespace", UNSET))

        def _parse_runtime_status(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        runtime_status = _parse_runtime_status(d.pop("runtime_status", UNSET))

        def _parse_setup_notes(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        setup_notes = _parse_setup_notes(d.pop("setup_notes", UNSET))

        installed_agent_view = cls(
            enabled=enabled,
            examples=examples,
            limitations=limitations,
            name=name,
            shared_memory_namespaces=shared_memory_namespaces,
            skills=skills,
            supports_a2a=supports_a2a,
            tags=tags,
            version=version,
            agent_class=agent_class,
            agent_type=agent_type,
            description=description,
            execution_mode=execution_mode,
            id=id,
            installed_at=installed_at,
            memory_namespace=memory_namespace,
            runtime_status=runtime_status,
            setup_notes=setup_notes,
        )

        installed_agent_view.additional_properties = d
        return installed_agent_view

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
