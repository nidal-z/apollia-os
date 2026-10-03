from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.input_kind import InputKind
from ..types import UNSET, Unset

T = TypeVar("T", bound="InputField")


@_attrs_define
class InputField:
    """One input of a workflow, as its launch form shows it.

    Attributes:
        kind (InputKind): The field a form shows for an input.
        name (str): The input's name, the key of the inputs object.
        nullable (bool): `true` when the input accepts null.
        required (bool): `true` when a launch without it is refused.
        choices (list[str] | Unset): The values a choice, or each item of a list of choices, may take, as
            text: a launch reads each back as the type the input declares.
        description (None | str | Unset): What the input is, as the workflow describes it.
        item_kind (InputKind | None | Unset):
    """

    kind: InputKind
    name: str
    nullable: bool
    required: bool
    choices: list[str] | Unset = UNSET
    description: None | str | Unset = UNSET
    item_kind: InputKind | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        kind = self.kind.value

        name = self.name

        nullable = self.nullable

        required = self.required

        choices: list[str] | Unset = UNSET
        if not isinstance(self.choices, Unset):
            choices = self.choices

        description: None | str | Unset
        if isinstance(self.description, Unset):
            description = UNSET
        else:
            description = self.description

        item_kind: None | str | Unset
        if isinstance(self.item_kind, Unset):
            item_kind = UNSET
        elif isinstance(self.item_kind, InputKind):
            item_kind = self.item_kind.value
        else:
            item_kind = self.item_kind

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "kind": kind,
                "name": name,
                "nullable": nullable,
                "required": required,
            }
        )
        if choices is not UNSET:
            field_dict["choices"] = choices
        if description is not UNSET:
            field_dict["description"] = description
        if item_kind is not UNSET:
            field_dict["itemKind"] = item_kind

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        kind = InputKind(d.pop("kind"))

        name = d.pop("name")

        nullable = d.pop("nullable")

        required = d.pop("required")

        choices = cast(list[str], d.pop("choices", UNSET))

        def _parse_description(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        description = _parse_description(d.pop("description", UNSET))

        def _parse_item_kind(data: object) -> InputKind | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, str):
                    raise TypeError()
                item_kind_type_1 = InputKind(data)

                return item_kind_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(InputKind | None | Unset, data)

        item_kind = _parse_item_kind(d.pop("itemKind", UNSET))

        input_field = cls(
            kind=kind,
            name=name,
            nullable=nullable,
            required=required,
            choices=choices,
            description=description,
            item_kind=item_kind,
        )

        input_field.additional_properties = d
        return input_field

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
