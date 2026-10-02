from __future__ import annotations

from collections.abc import Mapping
from typing import Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..types import UNSET, Unset

T = TypeVar("T", bound="LoadedModelDto")


@_attrs_define
class LoadedModelDto:
    """One resident model.

    Attributes:
        active_requests (int): Requests being answered right now. An unload is refused above zero.
        last_used_at (str): When a request last used the model, RFC 3339.
        loaded_at (str): When the model finished loading, RFC 3339.
        model (str): Display name, the model file name without its extension.
        model_path (str): Path the model was loaded from; the key an unload takes.
        n_ctx (int): Context window in tokens.
        n_gpu_layers (int): Layers placed on the GPU.
        port (int): Loopback port of the process serving it.
        n_parallel (int | None | Unset): Requests the process may serve in parallel; absent when the engine
            default applies.
    """

    active_requests: int
    last_used_at: str
    loaded_at: str
    model: str
    model_path: str
    n_ctx: int
    n_gpu_layers: int
    port: int
    n_parallel: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        active_requests = self.active_requests

        last_used_at = self.last_used_at

        loaded_at = self.loaded_at

        model = self.model

        model_path = self.model_path

        n_ctx = self.n_ctx

        n_gpu_layers = self.n_gpu_layers

        port = self.port

        n_parallel: int | None | Unset
        if isinstance(self.n_parallel, Unset):
            n_parallel = UNSET
        else:
            n_parallel = self.n_parallel

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "activeRequests": active_requests,
                "lastUsedAt": last_used_at,
                "loadedAt": loaded_at,
                "model": model,
                "modelPath": model_path,
                "nCtx": n_ctx,
                "nGpuLayers": n_gpu_layers,
                "port": port,
            }
        )
        if n_parallel is not UNSET:
            field_dict["nParallel"] = n_parallel

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        d = dict(src_dict)
        active_requests = d.pop("activeRequests")

        last_used_at = d.pop("lastUsedAt")

        loaded_at = d.pop("loadedAt")

        model = d.pop("model")

        model_path = d.pop("modelPath")

        n_ctx = d.pop("nCtx")

        n_gpu_layers = d.pop("nGpuLayers")

        port = d.pop("port")

        def _parse_n_parallel(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        n_parallel = _parse_n_parallel(d.pop("nParallel", UNSET))

        loaded_model_dto = cls(
            active_requests=active_requests,
            last_used_at=last_used_at,
            loaded_at=loaded_at,
            model=model,
            model_path=model_path,
            n_ctx=n_ctx,
            n_gpu_layers=n_gpu_layers,
            port=port,
            n_parallel=n_parallel,
        )

        loaded_model_dto.additional_properties = d
        return loaded_model_dto

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
