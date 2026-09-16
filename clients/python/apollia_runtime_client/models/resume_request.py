from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ResumeRequest")


@_attrs_define
class ResumeRequest:
    """Request body for `POST /api/v1/tasks/{id}/resume`.

    The operator submits a decision (`approved`) and an optional reason.
    The `approved` field is mandatory; omitting it produces HTTP 422.

        Attributes:
            approved (bool): `true` to approve, `false` to reject.
            answer (Any | Unset): The answer to a typed question: a proposition id, free text when the
                question allows it, or a value (a number for a `seuil`, a boolean for a
                `confirmation`).

                Checked against the pause it answers; a mismatch is a 422
                `INVALID_ANSWER`. Omitted, or `null`, for an approval and for a pause
                that carries a prompt alone.
            reason (None | str | Unset): Reason for the decision, optional, mainly useful when rejecting.
    """

    approved: bool
    answer: Any | Unset = UNSET
    reason: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        approved = self.approved

        answer = self.answer

        reason: None | str | Unset
        if isinstance(self.reason, Unset):
            reason = UNSET
        else:
            reason = self.reason

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "approved": approved,
            }
        )
        if answer is not UNSET:
            field_dict["answer"] = answer
        if reason is not UNSET:
            field_dict["reason"] = reason

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        approved = d.pop("approved")

        answer = d.pop("answer", UNSET)

        def _parse_reason(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        reason = _parse_reason(d.pop("reason", UNSET))

        resume_request = cls(
            approved=approved,
            answer=answer,
            reason=reason,
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
