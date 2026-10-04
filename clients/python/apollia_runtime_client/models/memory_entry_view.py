from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="MemoryEntryView")


@_attrs_define
class MemoryEntryView:
    """One memory entry, as the Memory screen lists it.

    Attributes:
        created_at (str): ISO 8601 creation date.
        entry_type (str): Type: `"episodic"` | `"semantic"` | `"procedural"`.
        id (str): Entry UUID.
        key (str): Key for semantic, trigger for procedural, agent id for episodic.
        value (str): Value or text content.
        expires_at (None | str | Unset): ISO 8601 expiration date, `null` without a TTL.
        score (float | None | Unset): BM25 score, `null` outside a search.
    """

    created_at: str
    entry_type: str
    id: str
    key: str
    value: str
    expires_at: None | str | Unset = UNSET
    score: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        created_at = self.created_at

        entry_type = self.entry_type

        id = self.id

        key = self.key

        value = self.value

        expires_at: None | str | Unset
        if isinstance(self.expires_at, Unset):
            expires_at = UNSET
        else:
            expires_at = self.expires_at

        score: float | None | Unset
        if isinstance(self.score, Unset):
            score = UNSET
        else:
            score = self.score

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "created_at": created_at,
                "entry_type": entry_type,
                "id": id,
                "key": key,
                "value": value,
            }
        )
        if expires_at is not UNSET:
            field_dict["expires_at"] = expires_at
        if score is not UNSET:
            field_dict["score"] = score

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        created_at = d.pop("created_at")

        entry_type = d.pop("entry_type")

        id = d.pop("id")

        key = d.pop("key")

        value = d.pop("value")

        def _parse_expires_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        expires_at = _parse_expires_at(d.pop("expires_at", UNSET))

        def _parse_score(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        score = _parse_score(d.pop("score", UNSET))

        memory_entry_view = cls(
            created_at=created_at,
            entry_type=entry_type,
            id=id,
            key=key,
            value=value,
            expires_at=expires_at,
            score=score,
        )

        memory_entry_view.additional_properties = d
        return memory_entry_view

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
