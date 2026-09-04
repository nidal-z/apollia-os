from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from typing import cast

if TYPE_CHECKING:
  from ..models.llm_backend_response_config_json import LlmBackendResponseConfigJson





T = TypeVar("T", bound="LlmBackendResponse")



@_attrs_define
class LlmBackendResponse:
    """ Response body for a single backend.

        Attributes:
            config_json (LlmBackendResponseConfigJson): Provider-specific configuration.
            enabled (bool): Whether this backend is enabled.
            is_default (bool): Whether this is the default backend.
            model (str): Model identifier.
            name (str): Unique backend name.
            provider (str): Provider identifier string.
     """

    config_json: LlmBackendResponseConfigJson
    enabled: bool
    is_default: bool
    model: str
    name: str
    provider: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.llm_backend_response_config_json import LlmBackendResponseConfigJson
        config_json = self.config_json.to_dict()

        enabled = self.enabled

        is_default = self.is_default

        model = self.model

        name = self.name

        provider = self.provider


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "config_json": config_json,
            "enabled": enabled,
            "is_default": is_default,
            "model": model,
            "name": name,
            "provider": provider,
        })

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.llm_backend_response_config_json import LlmBackendResponseConfigJson
        d = dict(src_dict)
        config_json = LlmBackendResponseConfigJson.from_dict(d.pop("config_json"))




        enabled = d.pop("enabled")

        is_default = d.pop("is_default")

        model = d.pop("model")

        name = d.pop("name")

        provider = d.pop("provider")

        llm_backend_response = cls(
            config_json=config_json,
            enabled=enabled,
            is_default=is_default,
            model=model,
            name=name,
            provider=provider,
        )


        llm_backend_response.additional_properties = d
        return llm_backend_response

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
