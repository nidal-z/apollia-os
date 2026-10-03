from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="MailboxMessageRow")


@_attrs_define
class MailboxMessageRow:
    """One inter-agent mailbox message, projected for the observability view.

    Attributes:
        from_agent (str): Sending agent name (or `host:<id>` for a host injection).
        message_id (str): Stable message identifier (primary key).
        payload (str): Raw JSON payload as stored, rendered verbatim in the UI.
        sent_at (str): Send timestamp (RFC 3339).
        state (str): Delivery state: `pending` or `in_flight`.
        to_agent (str): Receiving agent name.
    """

    from_agent: str
    message_id: str
    payload: str
    sent_at: str
    state: str
    to_agent: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from_agent = self.from_agent

        message_id = self.message_id

        payload = self.payload

        sent_at = self.sent_at

        state = self.state

        to_agent = self.to_agent

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "from_agent": from_agent,
                "message_id": message_id,
                "payload": payload,
                "sent_at": sent_at,
                "state": state,
                "to_agent": to_agent,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        from_agent = d.pop("from_agent")

        message_id = d.pop("message_id")

        payload = d.pop("payload")

        sent_at = d.pop("sent_at")

        state = d.pop("state")

        to_agent = d.pop("to_agent")

        mailbox_message_row = cls(
            from_agent=from_agent,
            message_id=message_id,
            payload=payload,
            sent_at=sent_at,
            state=state,
            to_agent=to_agent,
        )

        mailbox_message_row.additional_properties = d
        return mailbox_message_row

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
