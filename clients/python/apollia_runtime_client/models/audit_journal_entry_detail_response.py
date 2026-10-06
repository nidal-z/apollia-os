from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.audit_journal_entry_detail_response_entry import (
        AuditJournalEntryDetailResponseEntry,
    )


T = TypeVar("T", bound="AuditJournalEntryDetailResponse")


@_attrs_define
class AuditJournalEntryDetailResponse:
    """Response body for `GET /api/v1/audit/journal/:run_id/entries/:seq`.

    Attributes:
        entry (AuditJournalEntryDetailResponseEntry): The chained entry, with its `summary` and, when it is about a tool
            call, a `tool_call` holding the full `arguments`, `output` and `status`.
    """

    entry: AuditJournalEntryDetailResponseEntry
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        entry = self.entry.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "entry": entry,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.audit_journal_entry_detail_response_entry import (
            AuditJournalEntryDetailResponseEntry,
        )

        d = dict(src_dict)
        entry = AuditJournalEntryDetailResponseEntry.from_dict(d.pop("entry"))

        audit_journal_entry_detail_response = cls(
            entry=entry,
        )

        audit_journal_entry_detail_response.additional_properties = d
        return audit_journal_entry_detail_response

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
