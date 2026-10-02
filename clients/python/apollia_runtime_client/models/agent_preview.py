from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.access_preview import AccessPreview


T = TypeVar("T", bound="AgentPreview")


@_attrs_define
class AgentPreview:
    """One agent of the package.

    Attributes:
        access (list[AccessPreview]): Accesses the agent asks for, as governance resolves them.
        examples (list[str]): Example requests.
        id (str): Identifier.
        mission (str): What the agent is for.
        name (str): Display name.
    """

    access: list[AccessPreview]
    examples: list[str]
    id: str
    mission: str
    name: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        access = []
        for access_item_data in self.access:
            access_item = access_item_data.to_dict()
            access.append(access_item)

        examples = self.examples

        id = self.id

        mission = self.mission

        name = self.name

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "access": access,
                "examples": examples,
                "id": id,
                "mission": mission,
                "name": name,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.access_preview import AccessPreview

        d = dict(src_dict)
        access = []
        _access = d.pop("access")
        for access_item_data in _access:
            access_item = AccessPreview.from_dict(access_item_data)

            access.append(access_item)

        examples = cast(list[str], d.pop("examples"))

        id = d.pop("id")

        mission = d.pop("mission")

        name = d.pop("name")

        agent_preview = cls(
            access=access,
            examples=examples,
            id=id,
            mission=mission,
            name=name,
        )

        agent_preview.additional_properties = d
        return agent_preview

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
