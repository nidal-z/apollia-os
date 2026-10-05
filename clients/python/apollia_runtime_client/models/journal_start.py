from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

T = TypeVar("T", bound="JournalStart")


@_attrs_define
class JournalStart:
    """Where the verification of a purged journal starts: the first entry kept,
    which follows the anchor the last purge entry carries.

        Attributes:
            global_seq (int): Global sequence number of the first entry verified.
            purged_at (str): When the purge that set this start was made (RFC3339).
            ts (str): Date of the first entry verified (RFC3339).
    """

    global_seq: int
    purged_at: str
    ts: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        global_seq = self.global_seq

        purged_at = self.purged_at

        ts = self.ts

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "global_seq": global_seq,
                "purged_at": purged_at,
                "ts": ts,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        global_seq = d.pop("global_seq")

        purged_at = d.pop("purged_at")

        ts = d.pop("ts")

        journal_start = cls(
            global_seq=global_seq,
            purged_at=purged_at,
            ts=ts,
        )

        journal_start.additional_properties = d
        return journal_start

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
