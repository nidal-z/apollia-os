from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.locked_key_body_value import LockedKeyBodyValue


T = TypeVar("T", bound="LockedKeyBody")


@_attrs_define
class LockedKeyBody:
    """A key the organisation policy fixes, with its value.

    Attributes:
        key (str): Dotted key.
        value (LockedKeyBodyValue): The value the policy imposes.
    """

    key: str
    value: LockedKeyBodyValue
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        key = self.key

        value = self.value.to_dict()

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "key": key,
                "value": value,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.locked_key_body_value import LockedKeyBodyValue

        d = dict(src_dict)
        key = d.pop("key")

        value = LockedKeyBodyValue.from_dict(d.pop("value"))

        locked_key_body = cls(
            key=key,
            value=value,
        )

        locked_key_body.additional_properties = d
        return locked_key_body

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
