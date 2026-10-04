from __future__ import annotations

from collections.abc import Mapping
from typing import TYPE_CHECKING, Any, TypeVar, cast

from attrs import define as _attrs_define
from attrs import field as _attrs_field

from ..models.engine_name import EngineName
from ..types import UNSET, Unset

if TYPE_CHECKING:
    from ..models.guardrail_check_dto import GuardrailCheckDto
    from ..models.memory_estimate_dto import MemoryEstimateDto


T = TypeVar("T", bound="LoadEstimateResponse")


@_attrs_define
class LoadEstimateResponse:
    """Response of `POST /api/v1/llm/loaded-models/estimate`.

    Attributes:
        configurable (bool): Whether the load takes settings. `false` for an MLX model.
        engine (EngineName): An inference engine the store can hold.
        guardrail (GuardrailCheckDto): The load guardrail's view of a load.
        model_id (str): The model, as asked.
        model_path (str): The file or folder that would be loaded.
        n_ctx (int): Context window the process would be launched with.
        memory (MemoryEstimateDto | None | Unset):
        n_gpu_layers (int | None | Unset): Layers it would place on the accelerator; absent for an MLX model.
        total_layers (int | None | Unset): Layers the model has; absent when unknown.
    """

    configurable: bool
    engine: EngineName
    guardrail: GuardrailCheckDto
    model_id: str
    model_path: str
    n_ctx: int
    memory: MemoryEstimateDto | None | Unset = UNSET
    n_gpu_layers: int | None | Unset = UNSET
    total_layers: int | None | Unset = UNSET
    additional_properties: dict[str, Any] = _attrs_field(init=False, factory=dict)

    def to_dict(self) -> dict[str, Any]:
        from ..models.memory_estimate_dto import MemoryEstimateDto

        configurable = self.configurable

        engine = self.engine.value

        guardrail = self.guardrail.to_dict()

        model_id = self.model_id

        model_path = self.model_path

        n_ctx = self.n_ctx

        memory: dict[str, Any] | None | Unset
        if isinstance(self.memory, Unset):
            memory = UNSET
        elif isinstance(self.memory, MemoryEstimateDto):
            memory = self.memory.to_dict()
        else:
            memory = self.memory

        n_gpu_layers: int | None | Unset
        if isinstance(self.n_gpu_layers, Unset):
            n_gpu_layers = UNSET
        else:
            n_gpu_layers = self.n_gpu_layers

        total_layers: int | None | Unset
        if isinstance(self.total_layers, Unset):
            total_layers = UNSET
        else:
            total_layers = self.total_layers

        field_dict: dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "configurable": configurable,
                "engine": engine,
                "guardrail": guardrail,
                "modelId": model_id,
                "modelPath": model_path,
                "nCtx": n_ctx,
            }
        )
        if memory is not UNSET:
            field_dict["memory"] = memory
        if n_gpu_layers is not UNSET:
            field_dict["nGpuLayers"] = n_gpu_layers
        if total_layers is not UNSET:
            field_dict["totalLayers"] = total_layers

        return field_dict

    @classmethod
    def from_dict(cls: type[T], src_dict: Mapping[str, Any]) -> T:
        from ..models.guardrail_check_dto import GuardrailCheckDto
        from ..models.memory_estimate_dto import MemoryEstimateDto

        d = dict(src_dict)
        configurable = d.pop("configurable")

        engine = EngineName(d.pop("engine"))

        guardrail = GuardrailCheckDto.from_dict(d.pop("guardrail"))

        model_id = d.pop("modelId")

        model_path = d.pop("modelPath")

        n_ctx = d.pop("nCtx")

        def _parse_memory(data: object) -> MemoryEstimateDto | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            try:
                if not isinstance(data, dict):
                    raise TypeError()
                memory_type_1 = MemoryEstimateDto.from_dict(data)

                return memory_type_1
            except (TypeError, ValueError, AttributeError, KeyError):
                pass
            return cast(MemoryEstimateDto | None | Unset, data)

        memory = _parse_memory(d.pop("memory", UNSET))

        def _parse_n_gpu_layers(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        n_gpu_layers = _parse_n_gpu_layers(d.pop("nGpuLayers", UNSET))

        def _parse_total_layers(data: object) -> int | None | Unset:
            if data is None:
                return data
            if isinstance(data, Unset):
                return data
            return cast(int | None | Unset, data)

        total_layers = _parse_total_layers(d.pop("totalLayers", UNSET))

        load_estimate_response = cls(
            configurable=configurable,
            engine=engine,
            guardrail=guardrail,
            model_id=model_id,
            model_path=model_path,
            n_ctx=n_ctx,
            memory=memory,
            n_gpu_layers=n_gpu_layers,
            total_layers=total_layers,
        )

        load_estimate_response.additional_properties = d
        return load_estimate_response

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
