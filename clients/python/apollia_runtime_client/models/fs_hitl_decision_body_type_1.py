from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.fs_hitl_decision_body_type_1_decision import FsHitlDecisionBodyType1Decision
from ..types import UNSET, Unset

T = TypeVar("T", bound="FsHitlDecisionBodyType1")


@_attrs_define
class FsHitlDecisionBodyType1:
    """The user denies; the operation is cancelled and `reason`, when given,
    reaches the agent.

        Attributes:
            decision (FsHitlDecisionBodyType1Decision):
            reason (None | str | Unset): Free-text reason entered by the user.
    """

    decision: FsHitlDecisionBodyType1Decision
    reason: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        decision = self.decision.value

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "decision": decision,
            }
        )
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        decision = FsHitlDecisionBodyType1Decision(d.pop("decision"))

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        fs_hitl_decision_body_type_1 = cls(
            decision=decision,
            reason=reason,
        )

        fs_hitl_decision_body_type_1.additional_properties = d
        return fs_hitl_decision_body_type_1

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
