from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, BinaryIO, TextIO, TYPE_CHECKING, Generator

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

from ..types import UNSET, Unset
from typing import cast

if TYPE_CHECKING:
  from ..models.create_llm_backend_request_config_json import CreateLlmBackendRequestConfigJson





T = TypeVar("T", bound="CreateLlmBackendRequest")



@_attrs_define
class CreateLlmBackendRequest:
    """ Request body for `POST /api/v1/llm/backends`.

        Attributes:
            config_json (CreateLlmBackendRequestConfigJson): Provider-specific configuration object (must be a JSON object,
                not null or primitive).
            model (str): Model identifier (e.g. `"gpt-4o"`, `"mistral-small-latest"`).
            name (str): Unique name, pattern `^[a-z0-9_-]+$`.
            provider (str): Provider identifier: `"llama-cpp"`, `"openai"`, `"mistral"`, `"anthropic"`, `"ollama"`.
            enabled (bool | Unset): Whether this backend is active (default: `true`).
            is_default (bool | Unset): Mark this backend as the default (default: `false`).
     """

    config_json: CreateLlmBackendRequestConfigJson
    model: str
    name: str
    provider: str
    enabled: bool | Unset = UNSET
    is_default: bool | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)





    def to_dict(self) -> dict[str, Any]:
        from ..models.create_llm_backend_request_config_json import CreateLlmBackendRequestConfigJson
        config_json = self.config_json.to_dict()

        model = self.model

        name = self.name

        provider = self.provider

        enabled = self.enabled

        is_default = self.is_default


        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update({
            "config_json": config_json,
            "model": model,
            "name": name,
            "provider": provider,
        })
        if enabled is not UNSET:
            field_dict["enabled"] = enabled
        if is_default is not UNSET:
            field_dict["is_default"] = is_default

        return field_dict



    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.create_llm_backend_request_config_json import CreateLlmBackendRequestConfigJson
        d = dict(src_dict)
        config_json = CreateLlmBackendRequestConfigJson.from_dict(d.pop("config_json"))




        model = d.pop("model")

        name = d.pop("name")

        provider = d.pop("provider")

        enabled = d.pop("enabled", UNSET)

        is_default = d.pop("is_default", UNSET)

        create_llm_backend_request = cls(
            config_json=config_json,
            model=model,
            name=name,
            provider=provider,
            enabled=enabled,
            is_default=is_default,
        )


        create_llm_backend_request.additional_properties = d
        return create_llm_backend_request

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
