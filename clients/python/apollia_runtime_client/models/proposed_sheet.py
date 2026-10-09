from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ProposedSheet")


@_attrs_define
class ProposedSheet:
    """A sheet as it is proposed, before it is checked: every list optional.

    Attributes:
        mission (str): What the agent is for, used as its instructions.
        name (str): Display name.
        access (list[str] | Unset): Identifiers of the accesses asked for.
        examples (list[str] | Unset): Example requests.
        model (None | str | Unset): A model to assign; absent for the default model of the machine.
        sources (list[str] | Unset): Identifiers of the sources asked for.
    """

    mission: str
    name: str
    access: list[str] | Unset = UNSET
    examples: list[str] | Unset = UNSET
    model: None | str | Unset = UNSET
    sources: list[str] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        mission = self.mission

        name = self.name

        access: list[str] | Unset = UNSET
        if not isinstance(self.access, Unset):
            access = self.access

        examples: list[str] | Unset = UNSET
        if not isinstance(self.examples, Unset):
            examples = self.examples

        model: None | str | Unset
        if isinstance(self.model, Unset):
            model = UNSET
        else:
            model = self.model

        sources: list[str] | Unset = UNSET
        if not isinstance(self.sources, Unset):
            sources = self.sources

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "mission": mission,
                "name": name,
            }
        )
        if access is not UNSET:
            field_dict["access"] = access
        if examples is not UNSET:
            field_dict["examples"] = examples
        if model is not UNSET:
            field_dict["model"] = model
        if sources is not UNSET:
            field_dict["sources"] = sources

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        mission = d.pop("mission")

        name = d.pop("name")

        access = cast(list[str], d.pop("access", UNSET))

        examples = cast(list[str], d.pop("examples", UNSET))

        def _parse_model(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        model = _parse_model(d.pop("model", UNSET))

        sources = cast(list[str], d.pop("sources", UNSET))

        proposed_sheet = cls(
            mission=mission,
            name=name,
            access=access,
            examples=examples,
            model=model,
            sources=sources,
        )

        proposed_sheet.additional_properties = d
        return proposed_sheet

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
