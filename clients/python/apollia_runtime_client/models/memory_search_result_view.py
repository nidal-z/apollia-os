from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="MemorySearchResultView")


@_attrs_define
class MemorySearchResultView:
    """One full-text search result, as the Memory screen lists it.

    Attributes:
        content (str): Text content of the result.
        created_at (str): ISO 8601 creation date.
        entry_type (str): Type: `"episodic"` | `"semantic"`.
        id (str): UUID of the source entry.
        score (float): BM25 score, higher is more relevant.
        relevance (float | None | Unset): Importance (episodic) or confidence (semantic).
    """

    content: str
    created_at: str
    entry_type: str
    id: str
    score: float
    relevance: float | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        content = self.content

        created_at = self.created_at

        entry_type = self.entry_type

        id = self.id

        score = self.score

        relevance: float | None | Unset
        if isinstance(self.relevance, Unset):
            relevance = UNSET
        else:
            relevance = self.relevance

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "content": content,
                "created_at": created_at,
                "entry_type": entry_type,
                "id": id,
                "score": score,
            }
        )
        if relevance is not UNSET:
            field_dict["relevance"] = relevance

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        content = d.pop("content")

        created_at = d.pop("created_at")

        entry_type = d.pop("entry_type")

        id = d.pop("id")

        score = d.pop("score")

        def _parse_relevance(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        relevance = _parse_relevance(d.pop("relevance", UNSET))

        memory_search_result_view = cls(
            content=content,
            created_at=created_at,
            entry_type=entry_type,
            id=id,
            score=score,
            relevance=relevance,
        )

        memory_search_result_view.additional_properties = d
        return memory_search_result_view

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
