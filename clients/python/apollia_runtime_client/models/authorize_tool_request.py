from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="AuthorizeToolRequest")



@_attrs_define
class AuthorizeToolRequest:
    """ Request body for `POST /api/v1/sessions/:id/authorize`.

        Attributes:
            decision (str): Decision: `"accept"`, `"refuse"`, or `"always_accept"`.
            message_id (str): ID of the message that triggered the tool call.
            tool_name (str): Name of the tool.
            reason (None | str | Unset): Free-form rejection reason shared with the agent. Only honoured when
                `decision == "refuse"`.
            scope (None | str | Unset): Always-accept scope. Only honoured when `decision == "always_accept"`.
                Defaults to [`crate::chat::AlwaysAcceptScope::ThisSession`].
            tool_call_id (None | str | Unset): Unique id of the tool call being resolved. Correlates with the
                `approval_required` event so the same tool invoked twice in one turn
                resolves the right pending slot. Defaults to the tool name for legacy
                clients that do not yet send it.
     """

    decision: str
    message_id: str
    tool_name: str
    reason: None | str | Unset = UNSET
    scope: None | str | Unset = UNSET
    tool_call_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        decision = self.decision

        message_id = self.message_id

        tool_name = self.tool_name

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        scope: None | str | Unset
        if isinstance(self.scope, Unset):
            scope = UNSET
        else:
            scope = self.scope

        tool_call_id: None | str | Unset
        if isinstance(self.tool_call_id, Unset):
            tool_call_id = UNSET
        else:
            tool_call_id = self.tool_call_id


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "decision": decision,
            "message_id": message_id,
            "tool_name": tool_name,
        })
        if reason is not UNSET:
            field_dict["reason"] = reason
        if scope is not UNSET:
            field_dict["scope"] = scope
        if tool_call_id is not UNSET:
            field_dict["tool_call_id"] = tool_call_id

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        decision = d.pop("decision")

        message_id = d.pop("message_id")

        tool_name = d.pop("tool_name")

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))


        def _parse_scope(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        scope = _parse_scope(d.pop("scope", UNSET))


        def _parse_tool_call_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        tool_call_id = _parse_tool_call_id(d.pop("tool_call_id", UNSET))


        authorize_tool_request = cls(
            decision=decision,
            message_id=message_id,
            tool_name=tool_name,
            reason=reason,
            scope=scope,
            tool_call_id=tool_call_id,
        )


        authorize_tool_request.additional_properties = d
        return authorize_tool_request

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
