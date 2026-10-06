from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.pending_answer import PendingAnswer
from ..models.pending_kind import PendingKind
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.pending_decision_questions_type_0 import PendingDecisionQuestionsType0
    from ..models.pending_origin import PendingOrigin


T = TypeVar("T", bound="PendingDecision")


@_attrs_define
class PendingDecision:
    """One decision waiting for the user.

    Attributes:
        allow_other (bool): A free answer is taken besides the proposed choices.
        answers (list[PendingAnswer]): The answers it takes.
        id (str): Identifier to answer it by, stable while it waits.
        kind (PendingKind): Where a decision waiting for the user comes from.
        origin (PendingOrigin): Where the decision is to be found where it was born.
        summary (str): What is asked, in a sentence.
        access (None | str | Unset): The tool or the access asked for, for a consent and a batch item.
        agent_id (None | str | Unset): The agent that waits, when known.
        asked_at (None | str | Unset): When it was asked, RFC 3339, when known.
        family (None | str | Unset): Its action family (`read_personal`, `write_or_delete`, `send_external`,
            `execute_code`, `unclassified`), when known.
        questions (None | PendingDecisionQuestionsType0 | Unset): The questions of a question, each with its proposed
            choices, or the
            typed payload of a paused task.
    """

    allow_other: bool
    answers: list[PendingAnswer]
    id: str
    kind: PendingKind
    origin: PendingOrigin
    summary: str
    access: None | str | Unset = UNSET
    agent_id: None | str | Unset = UNSET
    asked_at: None | str | Unset = UNSET
    family: None | str | Unset = UNSET
    questions: None | PendingDecisionQuestionsType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.pending_decision_questions_type_0 import PendingDecisionQuestionsType0

        allow_other = self.allow_other

        answers = []
        for answers_item_data in self.answers:
            answers_item = answers_item_data.value
            answers.append(answers_item)

        id = self.id

        kind = self.kind.value

        origin = self.origin.to_dict()

        summary = self.summary

        access: None | str | Unset
        if isinstance(self.access, Unset):
            access = UNSET
        else:
            access = self.access

        agent_id: None | str | Unset
        if isinstance(self.agent_id, Unset):
            agent_id = UNSET
        else:
            agent_id = self.agent_id

        asked_at: None | str | Unset
        if isinstance(self.asked_at, Unset):
            asked_at = UNSET
        else:
            asked_at = self.asked_at

        family: None | str | Unset
        if isinstance(self.family, Unset):
            family = UNSET
        else:
            family = self.family

        questions: dict[str, Any] | None | Unset
        if isinstance(self.questions, Unset):
            questions = UNSET
        elif isinstance(self.questions, PendingDecisionQuestionsType0):
            questions = self.questions.to_dict()
        else:
            questions = self.questions

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "allowOther": allow_other,
                "answers": answers,
                "id": id,
                "kind": kind,
                "origin": origin,
                "summary": summary,
            }
        )
        if access is not UNSET:
            field_dict["access"] = access
        if agent_id is not UNSET:
            field_dict["agentId"] = agent_id
        if asked_at is not UNSET:
            field_dict["askedAt"] = asked_at
        if family is not UNSET:
            field_dict["family"] = family
        if questions is not UNSET:
            field_dict["questions"] = questions

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.pending_decision_questions_type_0 import PendingDecisionQuestionsType0
        from ..models.pending_origin import PendingOrigin

        d = dict(src_dict)
        allow_other = d.pop("allowOther")

        answers = []
        _answers = d.pop("answers")
        for answers_item_data in _answers:
            answers_item = PendingAnswer(answers_item_data)

            answers.append(answers_item)

        id = d.pop("id")

        kind = PendingKind(d.pop("kind"))

        origin = PendingOrigin.from_dict(d.pop("origin"))

        summary = d.pop("summary")

        def _parse_access(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        access = _parse_access(d.pop("access", UNSET))

        def _parse_agent_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        agent_id = _parse_agent_id(d.pop("agentId", UNSET))

        def _parse_asked_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        asked_at = _parse_asked_at(d.pop("askedAt", UNSET))

        def _parse_family(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        family = _parse_family(d.pop("family", UNSET))

        def _parse_questions(data: object) -> None | PendingDecisionQuestionsType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                questions_type_0 = PendingDecisionQuestionsType0.from_dict(data)

                return questions_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | PendingDecisionQuestionsType0 | Unset, data)

        questions = _parse_questions(d.pop("questions", UNSET))

        pending_decision = cls(
            allow_other=allow_other,
            answers=answers,
            id=id,
            kind=kind,
            origin=origin,
            summary=summary,
            access=access,
            agent_id=agent_id,
            asked_at=asked_at,
            family=family,
            questions=questions,
        )

        pending_decision.additional_properties = d
        return pending_decision

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
