from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="ConversationStatsView")


@_attrs_define
class ConversationStatsView:
    """Statistics of one chat conversation.

    Attributes:
        context_usage_pct (float): Approximate context window usage percentage.
        cross_sessions_referenced (int): Number of past sessions referenced via cross-session recall.
        message_count (int): Total number of messages in the session.
        summarized_count (int): Number of messages that have been summarized.
        user_memory_injected (bool): Whether the assistant's prompt carries the profile of the conversation.
    """

    context_usage_pct: float
    cross_sessions_referenced: int
    message_count: int
    summarized_count: int
    user_memory_injected: bool
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        context_usage_pct = self.context_usage_pct

        cross_sessions_referenced = self.cross_sessions_referenced

        message_count = self.message_count

        summarized_count = self.summarized_count

        user_memory_injected = self.user_memory_injected

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "context_usage_pct": context_usage_pct,
                "cross_sessions_referenced": cross_sessions_referenced,
                "message_count": message_count,
                "summarized_count": summarized_count,
                "user_memory_injected": user_memory_injected,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        context_usage_pct = d.pop("context_usage_pct")

        cross_sessions_referenced = d.pop("cross_sessions_referenced")

        message_count = d.pop("message_count")

        summarized_count = d.pop("summarized_count")

        user_memory_injected = d.pop("user_memory_injected")

        conversation_stats_view = cls(
            context_usage_pct=context_usage_pct,
            cross_sessions_referenced=cross_sessions_referenced,
            message_count=message_count,
            summarized_count=summarized_count,
            user_memory_injected=user_memory_injected,
        )

        conversation_stats_view.additional_properties = d
        return conversation_stats_view

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
