from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.connection_reason_type_0_key import ConnectionReasonType0Key
from ..types import UNSET, Unset

T = TypeVar("T", bound="ConnectionReasonType0")


@_attrs_define
class ConnectionReasonType0:
    """Why it is not working, absent when it works: a key the interface translates, the technical detail, and the address
    the repairing action opens when it has one.

        Attributes:
            key (ConnectionReasonType0Key):
            address (str | Unset): The address the repairing action opens, to be copied: for an administrator approval, the
                approval page.
            detail (None | str | Unset):
    """

    key: ConnectionReasonType0Key
    address: str | Unset = UNSET
    detail: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        key = self.key.value

        address = self.address

        detail: None | str | Unset
        if isinstance(self.detail, Unset):
            detail = UNSET
        else:
            detail = self.detail

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "key": key,
            }
        )
        if address is not UNSET:
            field_dict["address"] = address
        if detail is not UNSET:
            field_dict["detail"] = detail

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        key = ConnectionReasonType0Key(d.pop("key"))

        address = d.pop("address", UNSET)

        def _parse_detail(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        detail = _parse_detail(d.pop("detail", UNSET))

        connection_reason_type_0 = cls(
            key=key,
            address=address,
            detail=detail,
        )

        connection_reason_type_0.additional_properties = d
        return connection_reason_type_0

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
