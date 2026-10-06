from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.server_response_extra_params_type_0 import ServerResponseExtraParamsType0


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
        extra_params (None | ServerResponseExtraParamsType0 | Unset): Fields added to every request sent to this server.
            Absent when none.
        timeout_secs (int | None | Unset): Seconds the server may stay silent before a request to it is given
            up. Absent when the default of the runtime applies.
    """

    base_url: str
    enabled: bool
    has_key: bool
    name: str
    api_key_env: None | str | Unset = UNSET
    extra_params: None | ServerResponseExtraParamsType0 | Unset = UNSET
    timeout_secs: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.server_response_extra_params_type_0 import ServerResponseExtraParamsType0

        base_url = self.base_url

        enabled = self.enabled

        has_key = self.has_key

        name = self.name

        api_key_env: None | str | Unset
        if isinstance(self.api_key_env, Unset):
            api_key_env = UNSET
        else:
            api_key_env = self.api_key_env

        extra_params: dict[str, Any] | None | Unset
        if isinstance(self.extra_params, Unset):
            extra_params = UNSET
        elif isinstance(self.extra_params, ServerResponseExtraParamsType0):
            extra_params = self.extra_params.to_dict()
        else:
            extra_params = self.extra_params

        timeout_secs: int | None | Unset
        if isinstance(self.timeout_secs, Unset):
            timeout_secs = UNSET
        else:
            timeout_secs = self.timeout_secs

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
        if extra_params is not UNSET:
            field_dict["extra_params"] = extra_params
        if timeout_secs is not UNSET:
            field_dict["timeout_secs"] = timeout_secs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.server_response_extra_params_type_0 import ServerResponseExtraParamsType0

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

        def _parse_extra_params(data: object) -> None | ServerResponseExtraParamsType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                extra_params_type_0 = ServerResponseExtraParamsType0.from_dict(data)

                return extra_params_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | ServerResponseExtraParamsType0 | Unset, data)

        extra_params = _parse_extra_params(d.pop("extra_params", UNSET))

        def _parse_timeout_secs(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        timeout_secs = _parse_timeout_secs(d.pop("timeout_secs", UNSET))

        server_response = cls(
            base_url=base_url,
            enabled=enabled,
            has_key=has_key,
            name=name,
            api_key_env=api_key_env,
            extra_params=extra_params,
            timeout_secs=timeout_secs,
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
