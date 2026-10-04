from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.unread_reason_dto import UnreadReasonDto

T = TypeVar("T", bound="UnreadPageResponse")


@_attrs_define
class UnreadPageResponse:
    """A page of an indexed document that holds no text in the index.

    Attributes:
        page (int): Page number, from 1; a picture file is its page 1.
        path (str): Path of the document on this machine.
        reason (UnreadReasonDto): Why a page of an indexed document holds no text.
    """

    page: int
    path: str
    reason: UnreadReasonDto
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        page = self.page

        path = self.path

        reason = self.reason.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "page": page,
                "path": path,
                "reason": reason,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        page = d.pop("page")

        path = d.pop("path")

        reason = UnreadReasonDto(d.pop("reason"))

        unread_page_response = cls(
            page=page,
            path=path,
            reason=reason,
        )

        unread_page_response.additional_properties = d
        return unread_page_response

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
