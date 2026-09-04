from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..models.broken_link_reason import BrokenLinkReason






T = TypeVar("T", bound="BrokenLink")



@_attrs_define
class BrokenLink:
    """ The first broken link found while walking a chain.

        Attributes:
            reason (BrokenLinkReason): Reason a chain link failed verification.
            seq (int): Sequence number of the offending entry.
     """

    reason: BrokenLinkReason
    seq: int
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        reason = self.reason.value

        seq = self.seq


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "reason": reason,
            "seq": seq,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        reason = BrokenLinkReason(d.pop("reason"))




        seq = d.pop("seq")

        broken_link = cls(
            reason=reason,
            seq=seq,
        )


        broken_link.additional_properties = d
        return broken_link

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
