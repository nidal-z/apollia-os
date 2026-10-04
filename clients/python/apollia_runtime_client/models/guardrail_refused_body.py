from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.guardrail_level import GuardrailLevel

T = TypeVar("T", bound="GuardrailRefusedBody")


@_attrs_define
class GuardrailRefusedBody:
    """Body of an API answer to a launch the guardrail refused.

    `error` is the English sentence; `code` and the figures let an interface
    write its own, in its own language.

        Attributes:
            budget_gb (float): The limit the level holds to, in gigabytes.
            code (str): Always `guardrail_refused`.
            error (str): Human-readable description, in English.
            estimated_gb (float): The estimated footprint, in gigabytes.
            level (GuardrailLevel): How strictly a launch is held to the machine's memory.
            model (str): The model whose launch was refused.
    """

    budget_gb: float
    code: str
    error: str
    estimated_gb: float
    level: GuardrailLevel
    model: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        budget_gb = self.budget_gb

        code = self.code

        error = self.error

        estimated_gb = self.estimated_gb

        level = self.level.value

        model = self.model

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "budgetGb": budget_gb,
                "code": code,
                "error": error,
                "estimatedGb": estimated_gb,
                "level": level,
                "model": model,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        budget_gb = d.pop("budgetGb")

        code = d.pop("code")

        error = d.pop("error")

        estimated_gb = d.pop("estimatedGb")

        level = GuardrailLevel(d.pop("level"))

        model = d.pop("model")

        guardrail_refused_body = cls(
            budget_gb=budget_gb,
            code=code,
            error=error,
            estimated_gb=estimated_gb,
            level=level,
            model=model,
        )

        guardrail_refused_body.additional_properties = d
        return guardrail_refused_body

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
