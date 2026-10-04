from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.pending_answer import PendingAnswer

T = TypeVar("T", bound="DecideResponse")


@_attrs_define
class DecideResponse:
    """What a decision did.

    Attributes:
        answer (PendingAnswer): An answer a decision takes.
        id (str): The item answered.
        message (str): What happened, to show the user.
        resumed (int): How many waiting runs, conversations or items went on.
    """

    answer: PendingAnswer
    id: str
    message: str
    resumed: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        answer = self.answer.value

        id = self.id

        message = self.message

        resumed = self.resumed

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "answer": answer,
                "id": id,
                "message": message,
                "resumed": resumed,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        answer = PendingAnswer(d.pop("answer"))

        id = d.pop("id")

        message = d.pop("message")

        resumed = d.pop("resumed")

        decide_response = cls(
            answer=answer,
            id=id,
            message=message,
            resumed=resumed,
        )

        decide_response.additional_properties = d
        return decide_response

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
