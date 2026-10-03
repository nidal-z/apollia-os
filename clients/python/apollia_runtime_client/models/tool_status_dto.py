from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.tool_status_dto_config_type_0 import ToolStatusDtoConfigType0


T = TypeVar("T", bound="ToolStatusDto")


@_attrs_define
class ToolStatusDto:
    """State of a native tool as displayed by the tools page.

    Attributes:
        credential_keys (list[str]): Names of the credentials configured for this tool (values are never
            returned).
        enabled (bool): `true` when the tool is active.
        name (str): Canonical tool name (e.g. `bash_executor`).
        active_backend (None | str | Unset): Backend actually used by the tool when applicable
            (e.g. `"duckduckgo"` or `"brave"` for `web_search`).
        config (None | ToolStatusDtoConfigType0 | Unset): Tool-specific JSON configuration, or `null`.
    """

    credential_keys: list[str]
    enabled: bool
    name: str
    active_backend: None | str | Unset = UNSET
    config: None | ToolStatusDtoConfigType0 | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.tool_status_dto_config_type_0 import ToolStatusDtoConfigType0

        credential_keys = self.credential_keys

        enabled = self.enabled

        name = self.name

        active_backend: None | str | Unset
        if isinstance(self.active_backend, Unset):
            active_backend = UNSET
        else:
            active_backend = self.active_backend

        config: dict[str, Any] | None | Unset
        if isinstance(self.config, Unset):
            config = UNSET
        elif isinstance(self.config, ToolStatusDtoConfigType0):
            config = self.config.to_dict()
        else:
            config = self.config

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "credential_keys": credential_keys,
                "enabled": enabled,
                "name": name,
            }
        )
        if active_backend is not UNSET:
            field_dict["active_backend"] = active_backend
        if config is not UNSET:
            field_dict["config"] = config

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.tool_status_dto_config_type_0 import ToolStatusDtoConfigType0

        d = dict(src_dict)
        credential_keys = cast(list[str], d.pop("credential_keys"))

        enabled = d.pop("enabled")

        name = d.pop("name")

        def _parse_active_backend(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        active_backend = _parse_active_backend(d.pop("active_backend", UNSET))

        def _parse_config(data: object) -> None | ToolStatusDtoConfigType0 | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                config_type_0 = ToolStatusDtoConfigType0.from_dict(data)

                return config_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(None | ToolStatusDtoConfigType0 | Unset, data)

        config = _parse_config(d.pop("config", UNSET))

        tool_status_dto = cls(
            credential_keys=credential_keys,
            enabled=enabled,
            name=name,
            active_backend=active_backend,
            config=config,
        )

        tool_status_dto.additional_properties = d
        return tool_status_dto

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
