from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.pending_answer import PendingAnswer
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.decide_request_answers_type_0_item import DecideRequestAnswersType0Item
    from ..models.decide_request_value_type_0 import DecideRequestValueType0


T = TypeVar("T", bound="DecideRequest")


@_attrs_define
class DecideRequest:
    """Body of `POST /api/v1/pending-decisions/decide`.

    Attributes:
        answer (PendingAnswer): An answer a decision takes.
        id (str): The `id` of the item, as listed.
        answers (list[DecideRequestAnswersType0Item] | None | Unset): The answers to a question, one per question,
            matched by its `id`.
        reason (None | str | Unset): Why, for a refusal; handed to the agent.
        value (DecideRequestValueType0 | None | Unset): The answer to the typed question of a paused task: a proposition
            id,
            free text when it allows it, or a value.
    """

    answer: PendingAnswer
    id: str
    answers: list[DecideRequestAnswersType0Item] | None | Unset = UNSET
    reason: None | str | Unset = UNSET
    value: DecideRequestValueType0 | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.decide_request_value_type_0 import DecideRequestValueType0

        answer = self.answer.value

        id = self.id

        answers: list[dict[str, Any]] | None | Unset
        if isinstance(self.answers, Unset):
            answers = UNSET
        elif isinstance(self.answers, list):
            answers = []
            for answers_type_0_item_data in self.answers:
                answers_type_0_item = answers_type_0_item_data.to_dict()
                answers.append(answers_type_0_item)

        else:
            answers = self.answers

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        value: dict[str, Any] | None | Unset
        if isinstance(self.value, Unset):
            value = UNSET
        elif isinstance(self.value, DecideRequestValueType0):
            value = self.value.to_dict()
        else:
            value = self.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "answer": answer,
                "id": id,
            }
        )
        if answers is not UNSET:
            field_dict["answers"] = answers
        if reason is not UNSET:
            field_dict["reason"] = reason
        if value is not UNSET:
            field_dict["value"] = value

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.decide_request_answers_type_0_item import DecideRequestAnswersType0Item
        from ..models.decide_request_value_type_0 import DecideRequestValueType0

        d = dict(src_dict)
        answer = PendingAnswer(d.pop("answer"))

        id = d.pop("id")

        def _parse_answers(data: object) -> list[DecideRequestAnswersType0Item] | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, list):
                    raise TypeError()
                answers_type_0 = []
                _answers_type_0 = data
                for answers_type_0_item_data in _answers_type_0:
                    answers_type_0_item = DecideRequestAnswersType0Item.from_dict(
                        answers_type_0_item_data
                    )

                    answers_type_0.append(answers_type_0_item)

                return answers_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(list[DecideRequestAnswersType0Item] | None | Unset, data)

        answers = _parse_answers(d.pop("answers", UNSET))

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        def _parse_value(data: object) -> DecideRequestValueType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                value_type_0 = DecideRequestValueType0.from_dict(data)

                return value_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(DecideRequestValueType0 | None | Unset, data)

        value = _parse_value(d.pop("value", UNSET))

        decide_request = cls(
            answer=answer,
            id=id,
            answers=answers,
            reason=reason,
            value=value,
        )

        decide_request.additional_properties = d
        return decide_request

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
