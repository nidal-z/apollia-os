from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.batch_item_view import BatchItemView
    from ..models.batch_view import BatchView


T = TypeVar("T", bound="BatchDetailView")


@_attrs_define
class BatchDetailView:
    """A batch with its items.

    Attributes:
        batch (BatchView): A batch: what it was launched with, its state and its counters.
        items (list[BatchItemView]):
    """

    batch: BatchView
    items: list[BatchItemView]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        batch = self.batch.to_dict()

        items = []
        for items_item_data in self.items:
            items_item = items_item_data.to_dict()
            items.append(items_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "batch": batch,
                "items": items,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.batch_item_view import BatchItemView
        from ..models.batch_view import BatchView

        d = dict(src_dict)
        batch = BatchView.from_dict(d.pop("batch"))

        items = []
        _items = d.pop("items")
        for items_item_data in _items:
            items_item = BatchItemView.from_dict(items_item_data)

            items.append(items_item)

        batch_detail_view = cls(
            batch=batch,
            items=items,
        )

        batch_detail_view.additional_properties = d
        return batch_detail_view

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
