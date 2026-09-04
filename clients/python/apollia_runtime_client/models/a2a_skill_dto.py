from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast






T = TypeVar("T", bound="A2ASkillDto")



@_attrs_define
class A2ASkillDto:
    """ Skill declared by an A2A agent.

        Attributes:
            description (str): Description of what the skill does.
            id (str): Unique skill identifier.
            input_modes (list[str]): Supported input modes (e.g. `["text", "data"]`).
            name (str): Human-readable skill name.
            output_modes (list[str]): Supported output modes (e.g. `["text", "file"]`).
     """

    description: str
    id: str
    input_modes: list[str]
    name: str
    output_modes: list[str]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        description = self.description

        id = self.id

        input_modes = self.input_modes



        name = self.name

        output_modes = self.output_modes




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "description": description,
            "id": id,
            "input_modes": input_modes,
            "name": name,
            "output_modes": output_modes,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        description = d.pop("description")

        id = d.pop("id")

        input_modes = cast(list[str], d.pop("input_modes"))


        name = d.pop("name")

        output_modes = cast(list[str], d.pop("output_modes"))


        a2a_skill_dto = cls(
            description=description,
            id=id,
            input_modes=input_modes,
            name=name,
            output_modes=output_modes,
        )


        a2a_skill_dto.additional_properties = d
        return a2a_skill_dto

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
