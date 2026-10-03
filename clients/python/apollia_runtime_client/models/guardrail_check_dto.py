from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.guardrail_level import GuardrailLevel
from ..models.guardrail_verdict_dto import GuardrailVerdictDto
from ..types import UNSET, Unset

T = TypeVar("T", bound="GuardrailCheckDto")


@_attrs_define
class GuardrailCheckDto:
    """The load guardrail's view of a load.

    Attributes:
        level (GuardrailLevel): How strictly a launch is held to the machine's memory.
        verdict (GuardrailVerdictDto): What the load guardrail makes of a load.
        budget_gb (float | None | Unset): The limit it holds the load to, in gigabytes; absent without an
            estimate.
    """

    level: GuardrailLevel
    verdict: GuardrailVerdictDto
    budget_gb: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        level = self.level.value

        verdict = self.verdict.value

        budget_gb: float | None | Unset
        if isinstance(self.budget_gb, Unset):
            budget_gb = UNSET
        else:
            budget_gb = self.budget_gb

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "level": level,
                "verdict": verdict,
            }
        )
        if budget_gb is not UNSET:
            field_dict["budgetGb"] = budget_gb

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        level = GuardrailLevel(d.pop("level"))

        verdict = GuardrailVerdictDto(d.pop("verdict"))

        def _parse_budget_gb(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        budget_gb = _parse_budget_gb(d.pop("budgetGb", UNSET))

        guardrail_check_dto = cls(
            level=level,
            verdict=verdict,
            budget_gb=budget_gb,
        )

        guardrail_check_dto.additional_properties = d
        return guardrail_check_dto

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
