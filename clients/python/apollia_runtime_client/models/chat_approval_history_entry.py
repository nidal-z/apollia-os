from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ChatApprovalHistoryEntry")


@_attrs_define
class ChatApprovalHistoryEntry:
    """One decision on a tool call inside a chat.

    Attributes:
        decision (str): Decision taken (`accept`, `refuse`, `always_accept`).
        message_id (str): Message the approval was attached to.
        resolved_at (str): ISO-8601 timestamp of the resolution.
        session_id (str): Session identifier.
        tool_name (str): Tool that required approval.
        reason (None | str | Unset): The user's refusal reason, only set for `refuse`.
    """

    decision: str
    message_id: str
    resolved_at: str
    session_id: str
    tool_name: str
    reason: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        decision = self.decision

        message_id = self.message_id

        resolved_at = self.resolved_at

        session_id = self.session_id

        tool_name = self.tool_name

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
                "message_id": message_id,
                "resolved_at": resolved_at,
                "session_id": session_id,
                "tool_name": tool_name,
            }
        )
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        decision = d.pop("decision")

        message_id = d.pop("message_id")

        resolved_at = d.pop("resolved_at")

        session_id = d.pop("session_id")

        tool_name = d.pop("tool_name")

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        chat_approval_history_entry = cls(
            decision=decision,
            message_id=message_id,
            resolved_at=resolved_at,
            session_id=session_id,
            tool_name=tool_name,
            reason=reason,
        )

        chat_approval_history_entry.additional_properties = d
        return chat_approval_history_entry

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
