from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="PurgeMemoryBody")


@_attrs_define
class PurgeMemoryBody:
    """Request body for `POST /api/v1/memory/namespaces/{namespace}/purge`.

    Attributes:
        older_than_days (int): Entries older than this many days are removed.
        memory_type (None | str | Unset): `episodic`, `semantic`, `procedural`; absent or any other value
            reaches every kind.
    """

    older_than_days: int
    memory_type: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        older_than_days = self.older_than_days

        memory_type: None | str | Unset
        if isinstance(self.memory_type, Unset):
            memory_type = UNSET
        else:
            memory_type = self.memory_type

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "olderThanDays": older_than_days,
            }
        )
        if memory_type is not UNSET:
            field_dict["memoryType"] = memory_type

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        older_than_days = d.pop("olderThanDays")

        def _parse_memory_type(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        memory_type = _parse_memory_type(d.pop("memoryType", UNSET))

        purge_memory_body = cls(
            older_than_days=older_than_days,
            memory_type=memory_type,
        )

        purge_memory_body.additional_properties = d
        return purge_memory_body

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
