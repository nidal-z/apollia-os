from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.audit_journal_page_response_entries_item import AuditJournalPageResponseEntriesItem





T = TypeVar("T", bound="AuditJournalPageResponse")



@_attrs_define
class AuditJournalPageResponse:
    """ Response body for `GET /api/v1/audit/journal`.

        Attributes:
            count (int): Number of entries in this page.
            entries (list[AuditJournalPageResponseEntriesItem]): Journal entries, newest global position first.
     """

    count: int
    entries: list[AuditJournalPageResponseEntriesItem]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.audit_journal_page_response_entries_item import AuditJournalPageResponseEntriesItem
        count = self.count

        entries = []
        for entries_item_data in self.entries:
            entries_item = entries_item_data.to_dict()
            entries.append(entries_item)




        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "count": count,
            "entries": entries,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.audit_journal_page_response_entries_item import AuditJournalPageResponseEntriesItem
        d = dict(src_dict)
        count = d.pop("count")

        entries = []
        _entries = d.pop("entries")
        for entries_item_data in (_entries):
            entries_item = AuditJournalPageResponseEntriesItem.from_dict(entries_item_data)



            entries.append(entries_item)


        audit_journal_page_response = cls(
            count=count,
            entries=entries,
        )


        audit_journal_page_response.additional_properties = d
        return audit_journal_page_response

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
