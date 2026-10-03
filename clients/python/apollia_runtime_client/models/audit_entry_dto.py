from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="AuditEntryDto")


@_attrs_define
class AuditEntryDto:
    """Entry in the immutable audit log of permission decisions.

    Attributes:
        decided_at (str): Decision date in ISO 8601 format.
        decision (str): Serialized decision (e.g. `"AutoAllowedSafeList"`).
        id (int): Auto-incremented identifier.
        tool_name (str): Name of the invoked tool.
        agent (None | str | Unset): Name of the agent behind the invocation, if provided.
        first_arg (None | str | Unset): First argument extracted from the invocation, when available.
        rule_id (int | None | Unset): Identifier of the rule that triggered the decision, when applicable.
        scope (None | str | Unset): Scope of the rule that decided, when relevant.
    """

    decided_at: str
    decision: str
    id: int
    tool_name: str
    agent: None | str | Unset = UNSET
    first_arg: None | str | Unset = UNSET
    rule_id: int | None | Unset = UNSET
    scope: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        decided_at = self.decided_at

        decision = self.decision

        id = self.id

        tool_name = self.tool_name

        agent: None | str | Unset
        if isinstance(self.agent, Unset):
            agent = UNSET
        else:
            agent = self.agent

        first_arg: None | str | Unset
        if isinstance(self.first_arg, Unset):
            first_arg = UNSET
        else:
            first_arg = self.first_arg

        rule_id: int | None | Unset
        if isinstance(self.rule_id, Unset):
            rule_id = UNSET
        else:
            rule_id = self.rule_id

        scope: None | str | Unset
        if isinstance(self.scope, Unset):
            scope = UNSET
        else:
            scope = self.scope

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "decided_at": decided_at,
                "decision": decision,
                "id": id,
                "tool_name": tool_name,
            }
        )
        if agent is not UNSET:
            field_dict["agent"] = agent
        if first_arg is not UNSET:
            field_dict["first_arg"] = first_arg
        if rule_id is not UNSET:
            field_dict["rule_id"] = rule_id
        if scope is not UNSET:
            field_dict["scope"] = scope

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        decided_at = d.pop("decided_at")

        decision = d.pop("decision")

        id = d.pop("id")

        tool_name = d.pop("tool_name")

        def _parse_agent(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent = _parse_agent(d.pop("agent", UNSET))

        def _parse_first_arg(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        first_arg = _parse_first_arg(d.pop("first_arg", UNSET))

        def _parse_rule_id(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        rule_id = _parse_rule_id(d.pop("rule_id", UNSET))

        def _parse_scope(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        scope = _parse_scope(d.pop("scope", UNSET))

        audit_entry_dto = cls(
            decided_at=decided_at,
            decision=decision,
            id=id,
            tool_name=tool_name,
            agent=agent,
            first_arg=first_arg,
            rule_id=rule_id,
            scope=scope,
        )

        audit_entry_dto.additional_properties = d
        return audit_entry_dto

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
