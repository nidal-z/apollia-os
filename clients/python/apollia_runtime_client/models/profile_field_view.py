from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ProfileFieldView")


@_attrs_define
class ProfileFieldView:
    """One field of a profile, with what a form needs to draw it.

    Attributes:
        help_en (str): English help or example.
        help_fr (str): French help or example.
        key (str): Storage key of the field.
        label_en (str): English label.
        label_fr (str): French label.
        long_text (bool): `true` when the field takes several lines.
        value (str): Current value, empty when the field is not filled.
    """

    help_en: str
    help_fr: str
    key: str
    label_en: str
    label_fr: str
    long_text: bool
    value: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        help_en = self.help_en

        help_fr = self.help_fr

        key = self.key

        label_en = self.label_en

        label_fr = self.label_fr

        long_text = self.long_text

        value = self.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "help_en": help_en,
                "help_fr": help_fr,
                "key": key,
                "label_en": label_en,
                "label_fr": label_fr,
                "long_text": long_text,
                "value": value,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        help_en = d.pop("help_en")

        help_fr = d.pop("help_fr")

        key = d.pop("key")

        label_en = d.pop("label_en")

        label_fr = d.pop("label_fr")

        long_text = d.pop("long_text")

        value = d.pop("value")

        profile_field_view = cls(
            help_en=help_en,
            help_fr=help_fr,
            key=key,
            label_en=label_en,
            label_fr=label_fr,
            long_text=long_text,
            value=value,
        )

        profile_field_view.additional_properties = d
        return profile_field_view

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
