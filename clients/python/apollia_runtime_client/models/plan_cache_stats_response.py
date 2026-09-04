from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast






T = TypeVar("T", bound="PlanCacheStatsResponse")



@_attrs_define
class PlanCacheStatsResponse:
    """ Response body for `GET /api/v1/plan-cache/stats`.

        Attributes:
            cache_hits (int): Total number of cache hits across all entries.
            hit_rate_pct (float): Hit rate as a percentage (0.0–100.0).
            total_entries (int): Number of cached plans.
            newest_entry_at (None | str | Unset): Timestamp of the newest cache entry, or `null`.
            oldest_entry_at (None | str | Unset): Timestamp of the oldest cache entry, or `null`.
     """

    cache_hits: int
    hit_rate_pct: float
    total_entries: int
    newest_entry_at: None | str | Unset = UNSET
    oldest_entry_at: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        cache_hits = self.cache_hits

        hit_rate_pct = self.hit_rate_pct

        total_entries = self.total_entries

        newest_entry_at: None | str | Unset
        if isinstance(self.newest_entry_at, Unset):
            newest_entry_at = UNSET
        else:
            newest_entry_at = self.newest_entry_at

        oldest_entry_at: None | str | Unset
        if isinstance(self.oldest_entry_at, Unset):
            oldest_entry_at = UNSET
        else:
            oldest_entry_at = self.oldest_entry_at


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "cache_hits": cache_hits,
            "hit_rate_pct": hit_rate_pct,
            "total_entries": total_entries,
        })
        if newest_entry_at is not UNSET:
            field_dict["newest_entry_at"] = newest_entry_at
        if oldest_entry_at is not UNSET:
            field_dict["oldest_entry_at"] = oldest_entry_at

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        cache_hits = d.pop("cache_hits")

        hit_rate_pct = d.pop("hit_rate_pct")

        total_entries = d.pop("total_entries")

        def _parse_newest_entry_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        newest_entry_at = _parse_newest_entry_at(d.pop("newest_entry_at", UNSET))


        def _parse_oldest_entry_at(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        oldest_entry_at = _parse_oldest_entry_at(d.pop("oldest_entry_at", UNSET))


        plan_cache_stats_response = cls(
            cache_hits=cache_hits,
            hit_rate_pct=hit_rate_pct,
            total_entries=total_entries,
            newest_entry_at=newest_entry_at,
            oldest_entry_at=oldest_entry_at,
        )


        plan_cache_stats_response.additional_properties = d
        return plan_cache_stats_response

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
