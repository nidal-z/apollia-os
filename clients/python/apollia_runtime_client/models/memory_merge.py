from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="MemoryMerge")


@_attrs_define
class MemoryMerge:
    """A memory space of a project merged into the space of the same name.

    Attributes:
        entries_added (int | Unset): Entries this run added; 0 when they were already there. Default: 0.
        from_ (str | Unset): The space of the project, `<project id>:<space>`. Default: ''.
        into (str | Unset): The space it was merged into. Default: ''.
    """

    entries_added: int | Unset = 0
    from_: str | Unset = ""
    into: str | Unset = ""
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        entries_added = self.entries_added

        from_ = self.from_

        into = self.into

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if entries_added is not UNSET:
            field_dict["entriesAdded"] = entries_added
        if from_ is not UNSET:
            field_dict["from"] = from_
        if into is not UNSET:
            field_dict["into"] = into

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        entries_added = d.pop("entriesAdded", UNSET)

        from_ = d.pop("from", UNSET)

        into = d.pop("into", UNSET)

        memory_merge = cls(
            entries_added=entries_added,
            from_=from_,
            into=into,
        )

        memory_merge.additional_properties = d
        return memory_merge

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
