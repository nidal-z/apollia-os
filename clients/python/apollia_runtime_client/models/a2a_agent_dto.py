from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.a2a_agent_dto_input_schema_type_0 import A2AAgentDtoInputSchemaType0
    from ..models.a2a_skill_dto import A2ASkillDto


T = TypeVar("T", bound="A2AAgentDto")


@_attrs_define
class A2AAgentDto:
    """Entry in the A2A agent list.

    Attributes:
        agent_id (str): Unique identifier: the registry UUID of an agent whose code declares
            skills, `agent:<id>` for an agent that takes a message,
            `workflow:<package>/<name>` for a workflow.
        kind (str): `skill` for an agent whose code declares skills, `agent` for an agent
            that takes a message, `workflow` for a workflow.
        name (str): Agent name as declared in its manifest.
        skills (list[A2ASkillDto]): Skills declared by this agent. An agent that takes a message and a
            workflow each carry one, the entry `invoke` takes.
        state (str): Current process state (`active`, `degraded`, etc.).
        version (str): Agent semver version.
        input_schema (A2AAgentDtoInputSchemaType0 | None | Unset): JSON Schema of the input of an agent that takes a
            message or of a
            workflow; `null` for an agent whose skills each declare their own.
    """

    agent_id: str
    kind: str
    name: str
    skills: list[A2ASkillDto]
    state: str
    version: str
    input_schema: A2AAgentDtoInputSchemaType0 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.a2a_agent_dto_input_schema_type_0 import A2AAgentDtoInputSchemaType0

        agent_id = self.agent_id

        kind = self.kind

        name = self.name

        skills = []
        for skills_item_data in self.skills:
            skills_item = skills_item_data.to_dict()
            skills.append(skills_item)

        state = self.state

        version = self.version

        input_schema: dict[str, Any] | None | Unset
        if isinstance(self.input_schema, Unset):
            input_schema = UNSET
        elif isinstance(self.input_schema, A2AAgentDtoInputSchemaType0):
            input_schema = self.input_schema.to_dict()
        else:
            input_schema = self.input_schema

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "agent_id": agent_id,
                "kind": kind,
                "name": name,
                "skills": skills,
                "state": state,
                "version": version,
            }
        )
        if input_schema is not UNSET:
            field_dict["input_schema"] = input_schema

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.a2a_agent_dto_input_schema_type_0 import A2AAgentDtoInputSchemaType0
        from ..models.a2a_skill_dto import A2ASkillDto

        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        kind = d.pop("kind")

        name = d.pop("name")

        skills = []
        _skills = d.pop("skills")
        for skills_item_data in _skills:
            skills_item = A2ASkillDto.from_dict(skills_item_data)

            skills.append(skills_item)

        state = d.pop("state")

        version = d.pop("version")

        def _parse_input_schema(data: object) -> A2AAgentDtoInputSchemaType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                input_schema_type_0 = A2AAgentDtoInputSchemaType0.from_dict(data)

                return input_schema_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(A2AAgentDtoInputSchemaType0 | None | Unset, data)

        input_schema = _parse_input_schema(d.pop("input_schema", UNSET))

        a2a_agent_dto = cls(
            agent_id=agent_id,
            kind=kind,
            name=name,
            skills=skills,
            state=state,
            version=version,
            input_schema=input_schema,
        )

        a2a_agent_dto.additional_properties = d
        return a2a_agent_dto

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
