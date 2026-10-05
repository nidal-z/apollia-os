from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.journal_notice_type_1_notice import JournalNoticeType1Notice

T = TypeVar("T", bound="JournalNoticeType1")


@_attrs_define
class JournalNoticeType1:
    """The daily export the organisation policy asks for could not be
    written.

        Attributes:
            detail (str): What the write failed on.
            directory (str): The folder the policy designates.
            notice (JournalNoticeType1Notice):
    """

    detail: str
    directory: str
    notice: JournalNoticeType1Notice
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        detail = self.detail

        directory = self.directory

        notice = self.notice.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "detail": detail,
                "directory": directory,
                "notice": notice,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        detail = d.pop("detail")

        directory = d.pop("directory")

        notice = JournalNoticeType1Notice(d.pop("notice"))

        journal_notice_type_1 = cls(
            detail=detail,
            directory=directory,
            notice=notice,
        )

        journal_notice_type_1.additional_properties = d
        return journal_notice_type_1

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
