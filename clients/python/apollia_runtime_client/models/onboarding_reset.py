from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OnboardingReset")


@_attrs_define
class OnboardingReset:
    """Answer of `POST /api/v1/onboarding/reset`.

    Attributes:
        removed_profile_entries (int): How many profile entries were removed.
    """

    removed_profile_entries: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        removed_profile_entries = self.removed_profile_entries

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "removed_profile_entries": removed_profile_entries,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        removed_profile_entries = d.pop("removed_profile_entries")

        onboarding_reset = cls(
            removed_profile_entries=removed_profile_entries,
        )

        onboarding_reset.additional_properties = d
        return onboarding_reset

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
