from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CoachingRequest")


@_attrs_define
class CoachingRequest:
    """The connector just added.

    Attributes:
        server_name (str): The server name, matched against well-known connectors.
        server_title (None | str | Unset): The title shown to the operator, the name when absent.
    """

    server_name: str
    server_title: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        server_name = self.server_name

        server_title: None | str | Unset
        if isinstance(self.server_title, Unset):
            server_title = UNSET
        else:
            server_title = self.server_title

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "serverName": server_name,
            }
        )
        if server_title is not UNSET:
            field_dict["serverTitle"] = server_title

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        server_name = d.pop("serverName")

        def _parse_server_title(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        server_title = _parse_server_title(d.pop("serverTitle", UNSET))

        coaching_request = cls(
            server_name=server_name,
            server_title=server_title,
        )

        coaching_request.additional_properties = d
        return coaching_request

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
