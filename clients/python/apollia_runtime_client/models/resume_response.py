from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset







T = TypeVar("T", bound="ResumeResponse")



@_attrs_define
class ResumeResponse:
    """ Response body for `POST /api/v1/tasks/{id}/resume`.

    Returned with HTTP 200 when the resume is recorded successfully.

        Attributes:
            approved (bool): Operator decision.
            status (str): New task status (`"working"` after approval or rejection).
            task_id (str): Identifier of the resumed task.
     """

    approved: bool
    status: str
    task_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        approved = self.approved

        status = self.status

        task_id = self.task_id


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "approved": approved,
            "status": status,
            "task_id": task_id,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        approved = d.pop("approved")

        status = d.pop("status")

        task_id = d.pop("task_id")

        resume_response = cls(
            approved=approved,
            status=status,
            task_id=task_id,
        )


        resume_response.additional_properties = d
        return resume_response

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
