from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="CreateServerRequest")


@_attrs_define
class CreateServerRequest:
    """Body of `POST /api/v1/llm/servers`.

    Attributes:
        base_url (str): Base address, with or without the trailing `/v1`.
        name (str): Unique name, pattern `^[a-z0-9_-]+$`.
        api_key (None | str | Unset): Key, when the server asks for one. Written to the secret store.
        enabled (bool | Unset): Whether the server's models are routable (default: `true`).
    """

    base_url: str
    name: str
    api_key: None | str | Unset = UNSET
    enabled: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        base_url = self.base_url

        name = self.name

        api_key: None | str | Unset
        if isinstance(self.api_key, Unset):
            api_key = UNSET
        else:
            api_key = self.api_key

        enabled = self.enabled

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "base_url": base_url,
                "name": name,
            }
        )
        if api_key is not UNSET:
            field_dict["api_key"] = api_key
        if enabled is not UNSET:
            field_dict["enabled"] = enabled

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        base_url = d.pop("base_url")

        name = d.pop("name")

        def _parse_api_key(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        api_key = _parse_api_key(d.pop("api_key", UNSET))

        enabled = d.pop("enabled", UNSET)

        create_server_request = cls(
            base_url=base_url,
            name=name,
            api_key=api_key,
            enabled=enabled,
        )

        create_server_request.additional_properties = d
        return create_server_request

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
