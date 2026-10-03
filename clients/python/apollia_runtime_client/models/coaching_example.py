from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="CoachingExample")


@_attrs_define
class CoachingExample:
    """One coaching example shown as a clickable card in the wizard's last step.

    Attributes:
        description (str): One-line description shown under the title.
        prompt (str): Pre-filled chat message sent when the user clicks "Try".
        title (str): Short actionable title (e.g. "Summarise my latest Notion page").
    """

    description: str
    prompt: str
    title: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        description = self.description

        prompt = self.prompt

        title = self.title

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "description": description,
                "prompt": prompt,
                "title": title,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        description = d.pop("description")

        prompt = d.pop("prompt")

        title = d.pop("title")

        coaching_example = cls(
            description=description,
            prompt=prompt,
            title=title,
        )

        coaching_example.additional_properties = d
        return coaching_example

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
