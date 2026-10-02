from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PendingUserInputView")


@_attrs_define
class PendingUserInputView:
    """Snapshot of a pending `ask_user` request, safe to send across the IPC boundary.

    Attributes:
        created_at (str): ISO-8601 timestamp when the request was registered.
        questions_json (str): Questions serialised as JSON (Vec<UserQuestion>).
        request_id (str): Unique request identifier.
        session_id (str): Chat session that owns this request.
        context (None | str | Unset): Optional context string from the agent.
    """

    created_at: str
    questions_json: str
    request_id: str
    session_id: str
    context: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        questions_json = self.questions_json

        request_id = self.request_id

        session_id = self.session_id

        context: None | str | Unset
        if isinstance(self.context, Unset):
            context = UNSET
        else:
            context = self.context

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "questions_json": questions_json,
                "request_id": request_id,
                "session_id": session_id,
            }
        )
        if context is not UNSET:
            field_dict["context"] = context

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("created_at")

        questions_json = d.pop("questions_json")

        request_id = d.pop("request_id")

        session_id = d.pop("session_id")

        def _parse_context(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        context = _parse_context(d.pop("context", UNSET))

        pending_user_input_view = cls(
            created_at=created_at,
            questions_json=questions_json,
            request_id=request_id,
            session_id=session_id,
            context=context,
        )

        pending_user_input_view.additional_properties = d
        return pending_user_input_view

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
