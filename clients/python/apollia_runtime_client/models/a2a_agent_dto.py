from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.a2a_skill_dto import A2ASkillDto





T = TypeVar("T", bound="A2AAgentDto")



@_attrs_define
class A2AAgentDto:
    """ Entry in the A2A agent list.

        Attributes:
            agent_id (str): Unique agent identifier (UUID v4).
            name (str): Agent name as declared in its manifest.
            skills (list[A2ASkillDto]): Skills declared by this agent.
            state (str): Current process state (`active`, `degraded`, etc.).
            version (str): Agent semver version.
     """

    agent_id: str
    name: str
    skills: list[A2ASkillDto]
    state: str
    version: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.a2a_skill_dto import A2ASkillDto
        agent_id = self.agent_id

        name = self.name

        skills = []
        for skills_item_data in self.skills:
            skills_item = skills_item_data.to_dict()
            skills.append(skills_item)



        state = self.state

        version = self.version


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_id": agent_id,
            "name": name,
            "skills": skills,
            "state": state,
            "version": version,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.a2a_skill_dto import A2ASkillDto
        d = dict(src_dict)
        agent_id = d.pop("agent_id")

        name = d.pop("name")

        skills = []
        _skills = d.pop("skills")
        for skills_item_data in (_skills):
            skills_item = A2ASkillDto.from_dict(skills_item_data)



            skills.append(skills_item)


        state = d.pop("state")

        version = d.pop("version")

        a2a_agent_dto = cls(
            agent_id=agent_id,
            name=name,
            skills=skills,
            state=state,
            version=version,
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
