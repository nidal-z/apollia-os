from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar

from attrs import define as _attrs_define
from attrs import field as _attrs_field

if TYPE_CHECKING:
    from ..models.server_model import ServerModel


T = TypeVar("T", bound="ServerModelsResponse")


@_attrs_define
class ServerModelsResponse:
    """Body of `GET /api/v1/llm/servers/{name}/models`.

    Attributes:
        expected_context_window (int): Window an agent is expected to have, in tokens: a model served with a
            shorter one is marked `context_short`.
        models (list[ServerModel]): Models the server offers, in its order.
        server (str): Server name.
    """

    expected_context_window: int
    models: list[ServerModel]
    server: str
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        expected_context_window = self.expected_context_window

        models = []
        for models_item_data in self.models:
            models_item = models_item_data.to_dict()
            models.append(models_item)

        server = self.server

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "expected_context_window": expected_context_window,
                "models": models,
                "server": server,
            }
        )

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.server_model import ServerModel

        d = dict(src_dict)
        expected_context_window = d.pop("expected_context_window")

        models = []
        _models = d.pop("models")
        for models_item_data in _models:
            models_item = ServerModel.from_dict(models_item_data)

            models.append(models_item)

        server = d.pop("server")

        server_models_response = cls(
            expected_context_window=expected_context_window,
            models=models,
            server=server,
        )

        server_models_response.additional_properties = d
        return server_models_response

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
