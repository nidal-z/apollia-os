from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.a2a_skill_dto_input_schema_type_0 import A2ASkillDtoInputSchemaType0


T = TypeVar("T", bound="A2ASkillDto")


@_attrs_define
class A2ASkillDto:
    """Skill declared by an A2A agent.

    Attributes:
        description (str): Description of what the skill does.
        id (str): Unique skill identifier.
        input_modes (list[str]): Supported input modes (e.g. `["text", "data"]`).
        kind (str): `skill`, `agent` or `workflow`: what `id` launches through
            `POST /api/v1/a2a/invoke`.
        name (str): Human-readable skill name.
        output_modes (list[str]): Supported output modes (e.g. `["text", "file"]`).
        input_schema (A2ASkillDtoInputSchemaType0 | None | Unset): JSON Schema of the input `invoke` takes, when the
            entry declares one.
    """

    description: str
    id: str
    input_modes: list[str]
    kind: str
    name: str
    output_modes: list[str]
    input_schema: A2ASkillDtoInputSchemaType0 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.a2a_skill_dto_input_schema_type_0 import A2ASkillDtoInputSchemaType0

        description = self.description

        id = self.id

        input_modes = self.input_modes

        kind = self.kind

        name = self.name

        output_modes = self.output_modes

        input_schema: dict[str, Any] | None | Unset
        if isinstance(self.input_schema, Unset):
            input_schema = UNSET
        elif isinstance(self.input_schema, A2ASkillDtoInputSchemaType0):
            input_schema = self.input_schema.to_dict()
        else:
            input_schema = self.input_schema

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "description": description,
                "id": id,
                "input_modes": input_modes,
                "kind": kind,
                "name": name,
                "output_modes": output_modes,
            }
        )
        if input_schema is not UNSET:
            field_dict["input_schema"] = input_schema

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.a2a_skill_dto_input_schema_type_0 import A2ASkillDtoInputSchemaType0

        d = dict(src_dict)
        description = d.pop("description")

        id = d.pop("id")

        input_modes = cast(list[str], d.pop("input_modes"))

        kind = d.pop("kind")

        name = d.pop("name")

        output_modes = cast(list[str], d.pop("output_modes"))

        def _parse_input_schema(data: object) -> A2ASkillDtoInputSchemaType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                input_schema_type_0 = A2ASkillDtoInputSchemaType0.from_dict(data)

                return input_schema_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(A2ASkillDtoInputSchemaType0 | None | Unset, data)

        input_schema = _parse_input_schema(d.pop("input_schema", UNSET))

        a2a_skill_dto = cls(
            description=description,
            id=id,
            input_modes=input_modes,
            kind=kind,
            name=name,
            output_modes=output_modes,
            input_schema=input_schema,
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
