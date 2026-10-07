from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.update_server_request_extra_params_type_0 import (
        UpdateServerRequestExtraParamsType0,
    )


T = TypeVar("T", bound="UpdateServerRequest")


@_attrs_define
class UpdateServerRequest:
    """Body of `PATCH /api/v1/llm/servers/{name}`. A field left out keeps what
    the server has.

        Attributes:
            api_key (None | str | Unset): New key, written to the secret store. `null` or an empty string
                removes the key the server had.
            base_url (None | str | Unset): New base address, with or without the trailing `/v1`.
            enabled (bool | None | Unset): Whether the server's models are routable.
            extra_params (None | Unset | UpdateServerRequestExtraParamsType0): JSON object whose fields are added to every
                request sent to this
                server; a field the request already carries is left as it is. `model`,
                `messages`, `stream`, `stream_options`, `tools` and `tool_choice` are
                refused. `null` adds nothing.
            timeout_secs (int | None | Unset): Seconds the server may stay silent before a request to it is given
                up, 60 to 3600. `null` returns to the default of the runtime.
    """

    api_key: None | str | Unset = UNSET
    base_url: None | str | Unset = UNSET
    enabled: bool | None | Unset = UNSET
    extra_params: None | Unset | UpdateServerRequestExtraParamsType0 = UNSET
    timeout_secs: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.update_server_request_extra_params_type_0 import (
            UpdateServerRequestExtraParamsType0,
        )

        api_key: None | str | Unset
        if isinstance(self.api_key, Unset):
            api_key = UNSET
        else:
            api_key = self.api_key

        base_url: None | str | Unset
        if isinstance(self.base_url, Unset):
            base_url = UNSET
        else:
            base_url = self.base_url

        enabled: bool | None | Unset
        if isinstance(self.enabled, Unset):
            enabled = UNSET
        else:
            enabled = self.enabled

        extra_params: dict[str, Any] | None | Unset
        if isinstance(self.extra_params, Unset):
            extra_params = UNSET
        elif isinstance(self.extra_params, UpdateServerRequestExtraParamsType0):
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
        field_dict.update({})
        if api_key is not UNSET:
            field_dict["api_key"] = api_key
        if base_url is not UNSET:
            field_dict["base_url"] = base_url
        if enabled is not UNSET:
            field_dict["enabled"] = enabled
        if extra_params is not UNSET:
            field_dict["extra_params"] = extra_params
        if timeout_secs is not UNSET:
            field_dict["timeout_secs"] = timeout_secs

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.update_server_request_extra_params_type_0 import (
            UpdateServerRequestExtraParamsType0,
        )

        d = dict(src_dict)

        def _parse_api_key(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        api_key = _parse_api_key(d.pop("api_key", UNSET))

        def _parse_base_url(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        base_url = _parse_base_url(d.pop("base_url", UNSET))

        def _parse_enabled(data: object) -> bool | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(bool | None | Unset, data)

        enabled = _parse_enabled(d.pop("enabled", UNSET))

        def _parse_extra_params(data: object) -> None | Unset | UpdateServerRequestExtraParamsType0:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                extra_params_type_0 = UpdateServerRequestExtraParamsType0.from_dict(data)

                return extra_params_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | Unset | UpdateServerRequestExtraParamsType0, data)

        extra_params = _parse_extra_params(d.pop("extra_params", UNSET))

        def _parse_timeout_secs(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        timeout_secs = _parse_timeout_secs(d.pop("timeout_secs", UNSET))

        update_server_request = cls(
            api_key=api_key,
            base_url=base_url,
            enabled=enabled,
            extra_params=extra_params,
            timeout_secs=timeout_secs,
        )

        update_server_request.additional_properties = d
        return update_server_request

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
