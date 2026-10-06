from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.write_destination import WriteDestination
from ..types import UNSET, Unset

T = TypeVar("T", bound="SourceWrite")


@_attrs_define
class SourceWrite:
    """What a launch writes back into its source.

    Attributes:
        columns (list[str]): The columns added, one per field of the answer, in order. An answer
            that is not an object of these fields is written whole in the first.
        destination (WriteDestination | Unset): Where the results of a batch are written.
    """

    columns: list[str]
    destination: WriteDestination | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        columns = self.columns

        destination: str | Unset = UNSET
        if not isinstance(self.destination, Unset):
            destination = self.destination.value

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "columns": columns,
            }
        )
        if destination is not UNSET:
            field_dict["destination"] = destination

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        columns = cast(list[str], d.pop("columns"))

        _destination = d.pop("destination", UNSET)
        destination: WriteDestination | Unset
        if isinstance(_destination, Unset):
            destination = UNSET
        else:
            destination = WriteDestination(_destination)

        source_write = cls(
            columns=columns,
            destination=destination,
        )

        source_write.additional_properties = d
        return source_write

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
