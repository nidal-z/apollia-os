from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.journal_notice_type_0_notice import JournalNoticeType0Notice

T = TypeVar("T", bound="JournalNoticeType0")


@_attrs_define
class JournalNoticeType0:
    """The journal opened without its signing key: its entries are chained,
    so an alteration still shows, and they are not signed.

        Attributes:
            notice (JournalNoticeType0Notice):
    """

    notice: JournalNoticeType0Notice
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        notice = self.notice.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "notice": notice,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        notice = JournalNoticeType0Notice(d.pop("notice"))

        journal_notice_type_0 = cls(
            notice=notice,
        )

        journal_notice_type_0.additional_properties = d
        return journal_notice_type_0

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
