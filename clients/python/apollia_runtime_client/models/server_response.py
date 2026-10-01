from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="ServerResponse")


@_attrs_define
class ServerResponse:
    """One declared server. The key is never part of it.

    Attributes:
        base_url (str): Base address.
        enabled (bool): Whether the server's models are routable.
        has_key (bool): Whether a key is held for this server.
        name (str): Unique name, the first segment of `<server>/<model>`.
        api_key_env (None | str | Unset): Environment variable holding the key, for a migrated `${VAR}` key.
    """

    base_url: str
    enabled: bool
    has_key: bool
    name: str
    api_key_env: None | str | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        base_url = self.base_url

        enabled = self.enabled

        has_key = self.has_key

        name = self.name

        api_key_env: None | str | Unset
        if isinstance(self.api_key_env, Unset):
            api_key_env = UNSET
        else:
            api_key_env = self.api_key_env

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "base_url": base_url,
                "enabled": enabled,
                "has_key": has_key,
                "name": name,
            }
        )
        if api_key_env is not UNSET:
            field_dict["api_key_env"] = api_key_env

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        base_url = d.pop("base_url")

        enabled = d.pop("enabled")

        has_key = d.pop("has_key")

        name = d.pop("name")

        def _parse_api_key_env(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        api_key_env = _parse_api_key_env(d.pop("api_key_env", UNSET))

        server_response = cls(
            base_url=base_url,
            enabled=enabled,
            has_key=has_key,
            name=name,
            api_key_env=api_key_env,
        )

        server_response.additional_properties = d
        return server_response

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
