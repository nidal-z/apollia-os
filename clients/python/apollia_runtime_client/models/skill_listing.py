from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.skill_listing_input_schema_type_0 import SkillListingInputSchemaType0





T = TypeVar("T", bound="SkillListing")



@_attrs_define
class SkillListing:
    """ Entry in the list of available skills.

    Returned by [`A2AInvoker::list_skills`] and used by `ctx.a2a_list_skills()`.

        Attributes:
            agent_name (str): Name of the agent that provides this skill.
            description (str): Skill description.
            skill_id (str): Skill identifier.
            skill_name (str): Human-readable skill name.
            input_schema (None | SkillListingInputSchemaType0 | Unset): Apollia schema for the payload fields (cf.
                `AgentSkill::input_schema`).
                Used by `generate_a2a_tool_specs` to expose the worker's real contract
                to the LLM (instead of a generic schema).
     """

    agent_name: str
    description: str
    skill_id: str
    skill_name: str
    input_schema: None | SkillListingInputSchemaType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.skill_listing_input_schema_type_0 import SkillListingInputSchemaType0
        agent_name = self.agent_name

        description = self.description

        skill_id = self.skill_id

        skill_name = self.skill_name

        input_schema: dict[str, Any] | None | Unset
        if isinstance(self.input_schema, Unset):
            input_schema = UNSET
        elif isinstance(self.input_schema, SkillListingInputSchemaType0):
            input_schema = self.input_schema.to_dict()
        else:
            input_schema = self.input_schema


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "agent_name": agent_name,
            "description": description,
            "skill_id": skill_id,
            "skill_name": skill_name,
        })
        if input_schema is not UNSET:
            field_dict["input_schema"] = input_schema

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.skill_listing_input_schema_type_0 import SkillListingInputSchemaType0
        d = dict(src_dict)
        agent_name = d.pop("agent_name")

        description = d.pop("description")

        skill_id = d.pop("skill_id")

        skill_name = d.pop("skill_name")

        def _parse_input_schema(data: object) -> None | SkillListingInputSchemaType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                input_schema_type_0 = SkillListingInputSchemaType0.from_dict(data)



                return input_schema_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | SkillListingInputSchemaType0 | Unset, data)

        input_schema = _parse_input_schema(d.pop("input_schema", UNSET))


        skill_listing = cls(
            agent_name=agent_name,
            description=description,
            skill_id=skill_id,
            skill_name=skill_name,
            input_schema=input_schema,
        )


        skill_listing.additional_properties = d
        return skill_listing

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
