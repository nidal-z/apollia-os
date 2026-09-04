from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.skill_listing import SkillListing





T = TypeVar("T", bound="A2ASkillsResponse")



@_attrs_define
class A2ASkillsResponse:
    """ Response body for `GET /api/v1/a2a/skills`.

        Attributes:
            skills (list[SkillListing]): Flat list of all available A2A skills.
     """

    skills: list[SkillListing]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.skill_listing import SkillListing
        skills = []
        for skills_item_data in self.skills:
            skills_item = skills_item_data.to_dict()
            skills.append(skills_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "skills": skills,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.skill_listing import SkillListing
        d = dict(src_dict)
        skills = []
        _skills = d.pop("skills")
        for skills_item_data in (_skills):
            skills_item = SkillListing.from_dict(skills_item_data)



            skills.append(skills_item)


        a2a_skills_response = cls(
            skills=skills,
        )


        a2a_skills_response.additional_properties = d
        return a2a_skills_response

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
