from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.fs_hitl_decision_body_type_2_decision import FsHitlDecisionBodyType2Decision
from ..types import UNSET, Unset

T = TypeVar("T", bound="FsHitlDecisionBodyType2")


@_attrs_define
class FsHitlDecisionBodyType2:
    """The user approves and installs an "always allow" rule.

    Attributes:
        decision (FsHitlDecisionBodyType2Decision):
        level (str): Risk level (`medium`, `high`...).
        op (str): Filesystem operation (`write`, `delete`...).
        scope (None | str | Unset): Breadth of the rule; absent means this session only.
    """

    decision: FsHitlDecisionBodyType2Decision
    level: str
    op: str
    scope: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        decision = self.decision.value

        level = self.level

        op = self.op

        scope: None | str | Unset
        if isinstance(self.scope, Unset):
            scope = UNSET
        else:
            scope = self.scope

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "decision": decision,
                "level": level,
                "op": op,
            }
        )
        if scope is not UNSET:
            field_dict["scope"] = scope

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        decision = FsHitlDecisionBodyType2Decision(d.pop("decision"))

        level = d.pop("level")

        op = d.pop("op")

        def _parse_scope(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        scope = _parse_scope(d.pop("scope", UNSET))

        fs_hitl_decision_body_type_2 = cls(
            decision=decision,
            level=level,
            op=op,
            scope=scope,
        )

        fs_hitl_decision_body_type_2.additional_properties = d
        return fs_hitl_decision_body_type_2

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
