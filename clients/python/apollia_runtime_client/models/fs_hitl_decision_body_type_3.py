from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.fs_hitl_decision_body_type_3_decision import FsHitlDecisionBodyType3Decision

T = TypeVar("T", bound="FsHitlDecisionBodyType3")


@_attrs_define
class FsHitlDecisionBodyType3:
    """Legacy form of `always_allow` with the session scope.

    Attributes:
        decision (FsHitlDecisionBodyType3Decision):
        level (str): Risk level.
        op (str): Filesystem operation.
    """

    decision: FsHitlDecisionBodyType3Decision
    level: str
    op: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        decision = self.decision.value

        level = self.level

        op = self.op

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "decision": decision,
                "level": level,
                "op": op,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        decision = FsHitlDecisionBodyType3Decision(d.pop("decision"))

        level = d.pop("level")

        op = d.pop("op")

        fs_hitl_decision_body_type_3 = cls(
            decision=decision,
            level=level,
            op=op,
        )

        fs_hitl_decision_body_type_3.additional_properties = d
        return fs_hitl_decision_body_type_3

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
