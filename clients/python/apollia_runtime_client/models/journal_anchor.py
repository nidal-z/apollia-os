from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="JournalAnchor")



@_attrs_define
class JournalAnchor:
    """ The exportable head anchor of the global chain.

    Printing and storing this off-machine is the only defense against truncation
    of the global tail once the signing key can be compromised.

        Attributes:
            global_hash (str): Global hash at that position: the head of the chain.
            global_seq (int): Highest committed global sequence number.
            updated_ts (str): When the anchor was last advanced (RFC3339).
            key_id (None | str | Unset): Signing key id in force, when the journal is signed.
     """

    global_hash: str
    global_seq: int
    updated_ts: str
    key_id: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        global_hash = self.global_hash

        global_seq = self.global_seq

        updated_ts = self.updated_ts

        key_id: None | str | Unset
        if isinstance(self.key_id, Unset):
            key_id = UNSET
        else:
            key_id = self.key_id


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "global_hash": global_hash,
            "global_seq": global_seq,
            "updated_ts": updated_ts,
        })
        if key_id is not UNSET:
            field_dict["key_id"] = key_id

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        global_hash = d.pop("global_hash")

        global_seq = d.pop("global_seq")

        updated_ts = d.pop("updated_ts")

        def _parse_key_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        key_id = _parse_key_id(d.pop("key_id", UNSET))


        journal_anchor = cls(
            global_hash=global_hash,
            global_seq=global_seq,
            updated_ts=updated_ts,
            key_id=key_id,
        )


        journal_anchor.additional_properties = d
        return journal_anchor

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
