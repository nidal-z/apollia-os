from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="ResumeRequest")



@_attrs_define
class ResumeRequest:
    """ Request body for `POST /api/v1/tasks/{id}/resume`.

    The operator submits a decision (`approved`) and an optional reason.
    The `approved` field is mandatory; omitting it produces HTTP 422.

        Attributes:
            approved (bool): `true` to approve, `false` to reject.
            reason (None | str | Unset): Reason for the decision, optional, mainly useful when rejecting.
            answer (Any | Unset): The answer to a typed question: a proposition id, free text when the question allows it, or a value (a number for a `seuil`, a boolean for a `confirmation`). Omitted, or `null`, for an approval and for a prompt-only pause.
     """

    approved: bool
    reason: None | str | Unset = UNSET
    answer: Any | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        approved = self.approved

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason


        answer: Any | Unset
        if isinstance(self.answer, Unset):
            answer = UNSET
        else:
            answer = self.answer

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "approved": approved,
        })
        if reason is not UNSET:
            field_dict["reason"] = reason

        if answer is not UNSET:
            field_dict["answer"] = answer
        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        approved = d.pop("approved")

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))


        def _parse_answer(data: object) -> Any | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(Any | Unset, data)

        answer = _parse_answer(d.pop("answer", UNSET))

        resume_request = cls(
            approved=approved,
            reason=reason,
            answer=answer,
        )


        resume_request.additional_properties = d
        return resume_request

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
