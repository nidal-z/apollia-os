from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.removed_item import RemovedItem


T = TypeVar("T", bound="ConnectionRemoval")


@_attrs_define
class ConnectionRemoval:
    """What a removal erased, or would erase.

    Attributes:
        connection (str): Identifier of the connection.
        dry_run (bool): Whether nothing was erased, the list saying what would be.
        left (list[RemovedItem]): What could not be erased; empty on a dry run.
        removed (list[RemovedItem]): What was erased, or would be.
    """

    connection: str
    dry_run: bool
    left: list[RemovedItem]
    removed: list[RemovedItem]
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        connection = self.connection

        dry_run = self.dry_run

        left = []
        for left_item_data in self.left:
            left_item = left_item_data.to_dict()
            left.append(left_item)

        removed = []
        for removed_item_data in self.removed:
            removed_item = removed_item_data.to_dict()
            removed.append(removed_item)

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "connection": connection,
                "dryRun": dry_run,
                "left": left,
                "removed": removed,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.removed_item import RemovedItem

        d = dict(src_dict)
        connection = d.pop("connection")

        dry_run = d.pop("dryRun")

        left = []
        _left = d.pop("left")
        for left_item_data in _left:
            left_item = RemovedItem.from_dict(left_item_data)

            left.append(left_item)

        removed = []
        _removed = d.pop("removed")
        for removed_item_data in _removed:
            removed_item = RemovedItem.from_dict(removed_item_data)

            removed.append(removed_item)

        connection_removal = cls(
            connection=connection,
            dry_run=dry_run,
            left=left,
            removed=removed,
        )

        connection_removal.additional_properties = d
        return connection_removal

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
