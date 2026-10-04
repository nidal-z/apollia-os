from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.profile_entry_view import ProfileEntryView
    from ..models.profile_field_view import ProfileFieldView
    from ..models.profile_summary_view import ProfileSummaryView


T = TypeVar("T", bound="ProfileDetailView")


@_attrs_define
class ProfileDetailView:
    """One profile in full: its summary, its fields in schema order, its facts.

    Attributes:
        facts (list[ProfileEntryView]): The facts retained about the user in this profile.
        fields (list[ProfileFieldView]): Every field of the schema, filled or not.
        profile (ProfileSummaryView): One profile in the list of the profile screen.
    """

    facts: list[ProfileEntryView]
    fields: list[ProfileFieldView]
    profile: ProfileSummaryView
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        facts = []
        for facts_item_data in self.facts:
            facts_item = facts_item_data.to_dict()
            facts.append(facts_item)

        fields = []
        for fields_item_data in self.fields:
            fields_item = fields_item_data.to_dict()
            fields.append(fields_item)

        profile = self.profile.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "facts": facts,
                "fields": fields,
                "profile": profile,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.profile_entry_view import ProfileEntryView
        from ..models.profile_field_view import ProfileFieldView
        from ..models.profile_summary_view import ProfileSummaryView

        d = dict(src_dict)
        facts = []
        _facts = d.pop("facts")
        for facts_item_data in _facts:
            facts_item = ProfileEntryView.from_dict(facts_item_data)

            facts.append(facts_item)

        fields = []
        _fields = d.pop("fields")
        for fields_item_data in _fields:
            fields_item = ProfileFieldView.from_dict(fields_item_data)

            fields.append(fields_item)

        profile = ProfileSummaryView.from_dict(d.pop("profile"))

        profile_detail_view = cls(
            facts=facts,
            fields=fields,
            profile=profile,
        )

        profile_detail_view.additional_properties = d
        return profile_detail_view

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
