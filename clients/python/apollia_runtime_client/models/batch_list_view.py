from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.batch_view import BatchView


T = TypeVar("T", bound="BatchListView")


@_attrs_define
class BatchListView:
    """The batches, newest first.

    Attributes:
        batches (list[BatchView]):
    """

    batches: list[BatchView]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batches = []
        for batches_item_data in self.batches:
            batches_item = batches_item_data.to_dict()
            batches.append(batches_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batches": batches,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_view import BatchView

        d = dict(src_dict)
        batches = []
        _batches = d.pop("batches")
        for batches_item_data in _batches:
            batches_item = BatchView.from_dict(batches_item_data)

            batches.append(batches_item)

        batch_list_view = cls(
            batches=batches,
        )

        batch_list_view.additional_properties = d
        return batch_list_view

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
