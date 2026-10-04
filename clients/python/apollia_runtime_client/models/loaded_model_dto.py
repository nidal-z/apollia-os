from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_name import EngineName
from ..models.model_kind import ModelKind
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.loaded_model_dto_capabilities_type_0 import LoadedModelDtoCapabilitiesType0


T = TypeVar("T", bound="LoadedModelDto")


@_attrs_define
class LoadedModelDto:
    """One resident model.

    Attributes:
        active_requests (int): Requests being answered right now. An unload is refused above zero.
        engine (EngineName): An inference engine the store can hold.
        kind (ModelKind): What a resident model serves.
        last_used_at (str): When a request last used the model, RFC 3339.
        loaded_at (str): When the model finished loading, RFC 3339.
        model (str): Display name, the model file name without its extension.
        model_path (str): Path the model was loaded from; the key an unload takes.
        n_ctx (int): Context window in tokens.
        n_gpu_layers (int): Layers placed on the GPU.
        port (int): Loopback port of the process serving it.
        capabilities (LoadedModelDtoCapabilitiesType0 | None | Unset): What the model can do, from the model index.
        footprint_gb (float | None | Unset): Memory the load guardrail estimated for it at launch, in gigabytes;
            absent when the launch was not estimated.
        model_id (None | str | Unset): Identifier of the model in the model index, the name an agent uses;
            absent for a file the index does not hold.
        n_parallel (int | None | Unset): Requests the process may serve in parallel; absent when the engine
            default applies.
        size_bytes (int | None | Unset): Size of its weights on disk, from the model index.
    """

    active_requests: int
    engine: EngineName
    kind: ModelKind
    last_used_at: str
    loaded_at: str
    model: str
    model_path: str
    n_ctx: int
    n_gpu_layers: int
    port: int
    capabilities: LoadedModelDtoCapabilitiesType0 | None | Unset = UNSET
    footprint_gb: float | None | Unset = UNSET
    model_id: None | str | Unset = UNSET
    n_parallel: int | None | Unset = UNSET
    size_bytes: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.loaded_model_dto_capabilities_type_0 import LoadedModelDtoCapabilitiesType0

        active_requests = self.active_requests

        engine = self.engine.value

        kind = self.kind.value

        last_used_at = self.last_used_at

        loaded_at = self.loaded_at

        model = self.model

        model_path = self.model_path

        n_ctx = self.n_ctx

        n_gpu_layers = self.n_gpu_layers

        port = self.port

        capabilities: dict[str, Any] | None | Unset
        if isinstance(self.capabilities, Unset):
            capabilities = UNSET
        elif isinstance(self.capabilities, LoadedModelDtoCapabilitiesType0):
            capabilities = self.capabilities.to_dict()
        else:
            capabilities = self.capabilities

        footprint_gb: float | None | Unset
        if isinstance(self.footprint_gb, Unset):
            footprint_gb = UNSET
        else:
            footprint_gb = self.footprint_gb

        model_id: None | str | Unset
        if isinstance(self.model_id, Unset):
            model_id = UNSET
        else:
            model_id = self.model_id

        n_parallel: int | None | Unset
        if isinstance(self.n_parallel, Unset):
            n_parallel = UNSET
        else:
            n_parallel = self.n_parallel

        size_bytes: int | None | Unset
        if isinstance(self.size_bytes, Unset):
            size_bytes = UNSET
        else:
            size_bytes = self.size_bytes

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "activeRequests": active_requests,
                "engine": engine,
                "kind": kind,
                "lastUsedAt": last_used_at,
                "loadedAt": loaded_at,
                "model": model,
                "modelPath": model_path,
                "nCtx": n_ctx,
                "nGpuLayers": n_gpu_layers,
                "port": port,
            }
        )
        if capabilities is not UNSET:
            field_dict["capabilities"] = capabilities
        if footprint_gb is not UNSET:
            field_dict["footprintGb"] = footprint_gb
        if model_id is not UNSET:
            field_dict["modelId"] = model_id
        if n_parallel is not UNSET:
            field_dict["nParallel"] = n_parallel
        if size_bytes is not UNSET:
            field_dict["sizeBytes"] = size_bytes

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.loaded_model_dto_capabilities_type_0 import LoadedModelDtoCapabilitiesType0

        d = dict(src_dict)
        active_requests = d.pop("activeRequests")

        engine = EngineName(d.pop("engine"))

        kind = ModelKind(d.pop("kind"))

        last_used_at = d.pop("lastUsedAt")

        loaded_at = d.pop("loadedAt")

        model = d.pop("model")

        model_path = d.pop("modelPath")

        n_ctx = d.pop("nCtx")

        n_gpu_layers = d.pop("nGpuLayers")

        port = d.pop("port")

        def _parse_capabilities(data: object) -> LoadedModelDtoCapabilitiesType0 | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                capabilities_type_0 = LoadedModelDtoCapabilitiesType0.from_dict(data)

                return capabilities_type_0
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(LoadedModelDtoCapabilitiesType0 | None | Unset, data)

        capabilities = _parse_capabilities(d.pop("capabilities", UNSET))

        def _parse_footprint_gb(data: object) -> float | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(float | None | Unset, data)

        footprint_gb = _parse_footprint_gb(d.pop("footprintGb", UNSET))

        def _parse_model_id(data: object) -> None | str | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(None | str | Unset, data)

        model_id = _parse_model_id(d.pop("modelId", UNSET))

        def _parse_n_parallel(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        n_parallel = _parse_n_parallel(d.pop("nParallel", UNSET))

        def _parse_size_bytes(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        size_bytes = _parse_size_bytes(d.pop("sizeBytes", UNSET))

        loaded_model_dto = cls(
            active_requests=active_requests,
            engine=engine,
            kind=kind,
            last_used_at=last_used_at,
            loaded_at=loaded_at,
            model=model,
            model_path=model_path,
            n_ctx=n_ctx,
            n_gpu_layers=n_gpu_layers,
            port=port,
            capabilities=capabilities,
            footprint_gb=footprint_gb,
            model_id=model_id,
            n_parallel=n_parallel,
            size_bytes=size_bytes,
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
