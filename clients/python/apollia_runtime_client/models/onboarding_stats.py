from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="OnboardingStats")


@_attrs_define
class OnboardingStats:
    """Cumulative statistics accumulated over the full onboarding flow.

    Attributes:
        actions_completed (int): Number of discrete actions completed by the user.
        companion_questions (int): Number of questions asked to the AI Companion.
        total_time_sec (int): Total time spent in the onboarding flow, in seconds.
    """

    actions_completed: int
    companion_questions: int
    total_time_sec: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        actions_completed = self.actions_completed

        companion_questions = self.companion_questions

        total_time_sec = self.total_time_sec

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "actions_completed": actions_completed,
                "companion_questions": companion_questions,
                "total_time_sec": total_time_sec,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        actions_completed = d.pop("actions_completed")

        companion_questions = d.pop("companion_questions")

        total_time_sec = d.pop("total_time_sec")

        onboarding_stats = cls(
            actions_completed=actions_completed,
            companion_questions=companion_questions,
            total_time_sec=total_time_sec,
        )

        onboarding_stats.additional_properties = d
        return onboarding_stats

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
