from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.next_steps_reply_steps_item import NextStepsReplyStepsItem


T = TypeVar("T", bound="NextStepsReply")


@_attrs_define
class NextStepsReply:
    """Up to three next steps cards, and whether a model wrote them.

    Attributes:
        from_llm (bool): `false` when the heuristic wrote them.
        steps (list[NextStepsReplyStepsItem]): The cards, in display order.
    """

    from_llm: bool
    steps: list[NextStepsReplyStepsItem]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from_llm = self.from_llm

        steps = []
        for steps_item_data in self.steps:
            steps_item = steps_item_data.to_dict()
            steps.append(steps_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "fromLlm": from_llm,
                "steps": steps,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.next_steps_reply_steps_item import NextStepsReplyStepsItem

        d = dict(src_dict)
        from_llm = d.pop("fromLlm")

        steps = []
        _steps = d.pop("steps")
        for steps_item_data in _steps:
            steps_item = NextStepsReplyStepsItem.from_dict(steps_item_data)

            steps.append(steps_item)

        next_steps_reply = cls(
            from_llm=from_llm,
            steps=steps,
        )

        next_steps_reply.additional_properties = d
        return next_steps_reply

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
