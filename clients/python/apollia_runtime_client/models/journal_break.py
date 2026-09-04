from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.journal_break_reason import JournalBreakReason






T = TypeVar("T", bound="JournalBreak")



@_attrs_define
class JournalBreak:
    """ The first broken link found while walking the global chain.

        Attributes:
            global_seq (int): Global sequence number of the offending entry.
            reason (JournalBreakReason): Reason a global-chain link failed whole-journal verification.
            run_id (str): Run the offending entry belongs to.
     """

    global_seq: int
    reason: JournalBreakReason
    run_id: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        global_seq = self.global_seq

        reason = self.reason.value

        run_id = self.run_id


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "global_seq": global_seq,
            "reason": reason,
            "run_id": run_id,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        global_seq = d.pop("global_seq")

        reason = JournalBreakReason(d.pop("reason"))




        run_id = d.pop("run_id")

        journal_break = cls(
            global_seq=global_seq,
            reason=reason,
            run_id=run_id,
        )


        journal_break.additional_properties = d
        return journal_break

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
