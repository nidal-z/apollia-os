from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="BatchRetry")


@_attrs_define
class BatchRetry:
    """What the failed items of a batch are run again with.

    Attributes:
        grants (list[str] | Unset): Accesses granted for the rest of the batch, on top of those granted
            at launch: an item that waited for one of them runs again with it.
    """

    grants: list[str] | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        grants: list[str] | Unset = UNSET
        if not isinstance(self.grants, Unset):
            grants = self.grants

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({})
        if grants is not UNSET:
            field_dict["grants"] = grants

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        grants = cast(list[str], d.pop("grants", UNSET))

        batch_retry = cls(
            grants=grants,
        )

        batch_retry.additional_properties = d
        return batch_retry

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
